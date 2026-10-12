-- Mute persists beside the volume it silences, so a restart keeps both.
ALTER TABLE app_state ADD COLUMN muted INTEGER NOT NULL DEFAULT 0;
