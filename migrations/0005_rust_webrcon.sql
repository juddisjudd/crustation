-- Rust servers move to websocket RCON, which keeps the panel's commands out of the console.
UPDATE servers SET command = replace(command, '+rcon.web 0', '+rcon.web 1') WHERE kind = 'rust';
