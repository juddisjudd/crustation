-- What the last run of a schedule did, or why it failed, for the schedules page.
ALTER TABLE schedules ADD COLUMN last_result TEXT;
