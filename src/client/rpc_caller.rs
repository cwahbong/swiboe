// Copyright (c) The Swiboe development team. All rights reserved.
// Licensed under the Apache License, Version 2.0. See LICENSE.txt
// in the project root for license information.

use ::error::{Error, Result};
use ::client::rpc;
use ::spinner;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

pub type CommandSender = mpsc::Sender<Command>;
pub enum Command {
    Quit,
    OutgoingCall(String, mpsc::Sender<::rpc::Response>, ::ipc::Message),
    OutgoingCallFinished(::rpc::Response),
    CancelOutgoingRpc(String),
    Send(::ipc::Message),
}

struct Receiver {
    commands: mpsc::Receiver<Command>,
}

impl Receiver {
    fn new(commands: mpsc::Receiver<Command>) -> Self {
        Receiver {
            commands: commands,
        }
    }
}

impl spinner::Receiver<Command> for Receiver {
    fn recv(&mut self,) -> Result<Command> {
        match self.commands.recv() {
            Ok(value) => Ok(value),
            Err(_) => Err(Error::Disconnected),
        }
    }
}

struct Handler {
    remote_procedures: HashMap<String, Arc<Box<rpc::server::Rpc>>>, // Pull out
    running_function_calls: HashMap<String, mpsc::Sender<::rpc::Response>>,
    send_queue: mpsc::Sender<::ipc::Message>,
}

impl Handler {
    pub fn new(send_queue: mpsc::Sender<::ipc::Message>) -> Self {
        Handler {
            remote_procedures: HashMap::new(), // Pull out
            running_function_calls: HashMap::new(),
            send_queue: send_queue,
        }
    }
}

impl spinner::Handler<Command> for Handler {
    fn handle(&mut self, command: Command) -> Result<spinner::Command> {
        match command {
            Command::Quit => Ok(spinner::Command::Quit),
            Command::NewRpc(name, rpc) => {
                self.remote_procedures.insert(name, Arc::new(rpc));
                Ok(spinner::Command::Continue)
            },
            Command::Send(message) => {
                try!(self.send_queue.send(message));
                Ok(spinner::Command::Continue)
            },
            Command::OutgoingCall(context, tx, message) => {
                self.running_function_calls.insert(context, tx);
                // NOCOM(#sirver): can the message be constructed here?
                try!(self.send_queue.send(message));
                Ok(spinner::Command::Continue)
            }
            Command::OutgoingCallFinished(rpc_response) => {
                // NOCOM(#sirver): if this is a streaming RPC, we should cancel the
                // RPC.
                // This will quietly drop any updates on functions that we no longer
                // know/care about.
                self.running_function_calls
                    .get(&rpc_response.context)
                    .map(|channel| {
                        // The other side of this channel might not exist anymore - we
                        // might have dropped the RPC already. Just ignore it.
                        let _ = channel.send(rpc_response);
                    });
                Ok(spinner::Command::Continue)
            }
            Command::CancelOutgoingRpc(context) => {
                let msg = ::ipc::Message::RpcCancel(::rpc::Cancel {
                    context: context,
                });
                try!(self.send_queue.send(msg));
                Ok(spinner::Command::Continue)
            }
        }
    }
}


pub fn spawn(commands: mpsc::Receiver<Command>,
             send_queue: mpsc::Sender<::ipc::Message>) -> thread::JoinHandle<()> {
    let receiver = Receiver::new(commands);
    let handler = Handler::new(send_queue);
    spinner::spawn(receiver, handler)
}
