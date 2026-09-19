use api::{ApiEvent, QueueEdit, QueueSnapshot, TrackInfo};
use gpui_kit::{App, Context, Entity, Subscription, Task};
use link::Link;

/// The whole queue in play order, refetched whenever its revision moves.
pub struct QueueModel {
    snapshot: QueueSnapshot,
    generation: u64,
    _fetch: Option<Task<()>>,
    _subscription: Subscription,
}

impl QueueModel {
    pub fn global(cx: &App) -> Entity<Self> {
        crate::handle(cx)
    }

    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let connection = Link::connection(cx);
        let subscription = cx.subscribe(&connection, |this, _, event: &ApiEvent, cx| match event {
            ApiEvent::QueueChanged { rev, .. } if *rev != this.snapshot.rev => this.refetch(cx),
            ApiEvent::Resync => this.refetch(cx),
            _ => {}
        });
        Self {
            snapshot: QueueSnapshot::default(),
            generation: 0,
            _fetch: None,
            _subscription: subscription,
        }
    }

    fn refetch(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let call = Link::global(cx).call(|api| async move { api.queue_snapshot().await });
        self._fetch = Some(cx.spawn(async move |this, cx| {
            let Ok(snapshot) = call.await else { return };
            this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.snapshot = snapshot;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Rows in play order: index 0 plays first.
    pub fn items(&self) -> &[TrackInfo] {
        &self.snapshot.items
    }

    /// Play-order index of the current row.
    pub fn position(&self) -> Option<u32> {
        self.snapshot.position
    }

    pub fn current(&self) -> Option<&TrackInfo> {
        self.snapshot
            .position
            .and_then(|position| self.snapshot.items.get(position as usize))
    }

    pub fn len(&self) -> usize {
        self.snapshot.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshot.items.is_empty()
    }

    pub fn shuffle(&self) -> bool {
        self.snapshot.shuffle
    }

    /// For each play-order position, its index in the unshuffled queue; empty while shuffle is off.
    pub fn shuffle_order(&self) -> &[u32] {
        &self.snapshot.shuffle_order
    }

    pub fn rev(&self) -> u64 {
        self.snapshot.rev
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.snapshot.items.iter().any(|item| item.key == key)
    }

    pub fn jump(&mut self, index: u32, cx: &mut Context<Self>) {
        self.snapshot.position = Some(index);
        cx.notify();
        edit(QueueEdit::Jump { index }, cx);
    }

    pub fn move_item(&mut self, from: u32, to: u32, cx: &mut Context<Self>) {
        let items = &mut self.snapshot.items;
        if (from as usize) < items.len() && (to as usize) < items.len() {
            let item = items.remove(from as usize);
            items.insert(to as usize, item);
            cx.notify();
        }
        edit(QueueEdit::Move { from, to }, cx);
    }

    pub fn remove(&mut self, index: u32, cx: &mut Context<Self>) {
        edit(QueueEdit::Remove { index }, cx);
    }

    pub fn insert(&mut self, index: u32, keys: Vec<String>, cx: &mut Context<Self>) {
        edit(QueueEdit::Insert { index, keys }, cx);
    }
}

fn edit(edit: QueueEdit, cx: &App) {
    Link::global(cx).fire(cx, "queue edit", move |api| async move {
        api.queue_edit(edit).await
    });
}
