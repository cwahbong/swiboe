// Copyright (c) The Swiboe development team. All rights reserved.
// Licensed under the Apache License, Version 2.0. See LICENSE.txt
// in the project root for license information.

use std;
use std::sync::mpsc;
use std::thread;


/// spawn a reader thread that reads message from a readable source to a mpsc sender.
pub fn spawn_reader<R, T>(src: R, dst: mpsc::Sender<::ipc::Message>) -> thread::JoinHandle<()>
        where R: std::io::Read + Send + 'static,
              T: Send + 'static {
    thread::spawn(move || {
        let mut reader = ::ipc::Reader::new(from);
        while let Ok(message) = reader.read_message() {
            if to.send(message).is_err() {
                break;
            }
        }
    })
}

/// spawn a writer thread that fetch messages with type T from a mpsc receiver to a writable
/// destination.
// XXX change ::ipc::Message -> T
pub fn spawn_writer<T, W>(src: mpsc::Receiver<::ipc::Message>, dst: W) -> thread::JoinHandle<()>
        where T: Send + 'static,
              W: std::io::Write + Send + 'static {
    thread::spawn(move || {
        let mut writer = ::ipc::Writer::new(to);
        while let Ok(message) = from.recv() {
            writer.write_message(&message).expect("Write fail");
        }
    })
}

// TODO move mio related and ipc_bridge from server::* to here
