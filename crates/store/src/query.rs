use std::future::Future;
use std::rc::Rc;
use std::sync::Arc;

use api::{ApiError, KopuzApi, Table};
use futures::FutureExt as _;
use futures::future::BoxFuture;
use gpui_kit::{Context, Subscription, Task};
use link::Link;

use crate::invalidation::{Invalidation, TablesChanged};

type Fetch<T> = Rc<dyn Fn(Arc<dyn KopuzApi>) -> BoxFuture<'static, Result<T, ApiError>>>;

/// One daemon read kept fresh: re-runs when its tables are invalidated, keeps the stale value while it does, and drops answers overtaken by a newer run.
pub struct Query<T> {
    fetch: Fetch<T>,
    tables: Vec<Table>,
    value: Option<T>,
    error: Option<ApiError>,
    loading: bool,
    generation: u64,
    _task: Option<Task<()>>,
    _subscription: Subscription,
}

impl<T: Send + 'static> Query<T> {
    /// `tables` are the invalidations that re-run `fetch`; a resync or source switch always does.
    pub fn new<Fut>(
        tables: &[Table],
        fetch: impl Fn(Arc<dyn KopuzApi>) -> Fut + 'static,
        cx: &mut Context<Self>,
    ) -> Self
    where
        Fut: Future<Output = Result<T, ApiError>> + Send + 'static,
    {
        let invalidation = Invalidation::global(cx);
        let subscription = cx.subscribe(&invalidation, |this, _, event: &TablesChanged, cx| {
            if event.touches(&this.tables) {
                this.refresh(cx);
            }
        });
        let mut query = Self {
            fetch: box_fetch(fetch),
            tables: tables.to_vec(),
            value: None,
            error: None,
            loading: false,
            generation: 0,
            _task: None,
            _subscription: subscription,
        };
        if Link::connection(cx).read(cx).is_connected() {
            query.refresh(cx);
        }
        query
    }

    /// Point the query at new inputs; the old value is dropped since it answers a different question.
    pub fn set_fetch<Fut>(
        &mut self,
        fetch: impl Fn(Arc<dyn KopuzApi>) -> Fut + 'static,
        cx: &mut Context<Self>,
    ) where
        Fut: Future<Output = Result<T, ApiError>> + Send + 'static,
    {
        self.fetch = box_fetch(fetch);
        self.value = None;
        self.error = None;
        self.refresh(cx);
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let fetch = self.fetch.clone();
        let call = Link::global(cx).call(move |api| fetch(api));
        self.loading = true;
        cx.notify();
        self._task = Some(cx.spawn(async move |this, cx| {
            let result = call.await;
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading = false;
                match result {
                    Ok(value) => {
                        this.value = Some(value);
                        this.error = None;
                    }
                    Err(error) => {
                        tracing::debug!(%error, "query failed");
                        this.error = Some(error);
                    }
                }
                cx.notify();
            })
            .ok();
        }));
    }
}

impl<T: 'static> Query<T> {
    pub fn value(&self) -> Option<&T> {
        self.value.as_ref()
    }

    pub fn error(&self) -> Option<&ApiError> {
        self.error.as_ref()
    }

    /// A run is in flight; the previous value, if any, is still readable.
    pub fn is_loading(&self) -> bool {
        self.loading
    }

    /// The first answer (or failure) has arrived.
    pub fn is_settled(&self) -> bool {
        self.value.is_some() || self.error.is_some()
    }

    /// Edit the held value in place, for an optimistic change the next refetch will confirm.
    pub fn update_value(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut T)) {
        if let Some(value) = self.value.as_mut() {
            edit(value);
            cx.notify();
        }
    }
}

fn box_fetch<T, Fut>(fetch: impl Fn(Arc<dyn KopuzApi>) -> Fut + 'static) -> Fetch<T>
where
    Fut: Future<Output = Result<T, ApiError>> + Send + 'static,
{
    Rc::new(move |api| fetch(api).boxed())
}
