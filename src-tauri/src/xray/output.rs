//! Forwarding of the child process output to the UI.

use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read},
    sync::{Arc, Mutex},
    thread,
};

use tauri::{AppHandle, Emitter};

use super::LOG_EVENT;

/// Recent output lines, so a failed start can explain itself.
pub(crate) type Tail = Arc<Mutex<VecDeque<String>>>;

pub(crate) fn stream_output<R>(app: AppHandle, reader: R)
where
    R: Read + Send + 'static,
{
    stream_output_tail(app, reader, Tail::default());
}

pub(crate) fn stream_output_tail<R>(app: AppHandle, reader: R, tail: Tail)
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            if let Ok(mut sink) = tail.lock() {
                sink.push_back(line.clone());
                while sink.len() > 8 {
                    sink.pop_front();
                }
            }
            let _ = app.emit(LOG_EVENT, line);
        }
    });
}
