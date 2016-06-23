// Copyright (c) The Swiboe development team. All rights reserved.
// Licensed under the Apache License, Version 2.0. See LICENSE.txt
// in the project root for license information.

use ::client::rpc;
use ::client::rpc_caller;
use ::error::{Error, Result};
use ::spinner;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use threadpool::ThreadPool;


struct RunningRpc {
    commands: mpsc::Sender<rpc::server::Command>,
}

impl RunningRpc {
    fn new(commands: mpsc::Sender<rpc::server::Command>) -> Self {
        RunningRpc {
            commands: commands,
        }
    }
}

struct Receiver {
    messages: mpsc::Receiver<::ipc::Message>
}

impl Receiver {
    pub fn new(messages: mpsc::Receiver<::ipc::Message>) -> Self {
        Receiver { messages: messages }
    }
}

impl spinner::Receiver<::ipc::Message> for Receiver {
    fn recv(&mut self) -> Result<::ipc::Message> {
        match self.messages.recv() {
            Ok(value) => Ok(value),
            Err(_) => Err(Error::Disconnected),
        }
    }
}

struct Handler {
    remote_procedures: HashMap<String, Arc<Box<rpc::server::Rpc>>>,
    running_rpc_calls: HashMap<String, RunningRpc>,
    caller_sender: mpsc::Sender<rpc_caller::Command>,
    // NOCOM(#sirver): maybe not use a channel to send data to rpcs?
    thread_pool: ThreadPool,
}

impl Handler {
    pub fn new(caller_sender: mpsc::Sender<rpc_caller::Command>) -> Self {
        Handler {
            remote_procedures: HashMap::new(),
            running_rpc_calls: HashMap::new(),
            caller_sender: caller_sender,
            // NOCOM(#sirver): that seems silly.
            thread_pool: ThreadPool::new(1),
        }
    }
}

impl spinner::Handler<::ipc::Message> for Handler {
    fn handle(&mut self, message: ::ipc::Message) -> Result<spinner::Command> {
        match message {
            ::ipc::Message::RpcCall(rpc_call) => {
                if let Some(function) = self.remote_procedures.get(&rpc_call.function) {
                    let (tx, rx) = mpsc::channel();
                    self.running_rpc_calls.insert(rpc_call.context.clone(), RunningRpc::new(tx));
                    let caller_sender = self.caller_sender.clone();
                    let function = function.clone();
                    self.thread_pool.execute(move || {
                        function.call(rpc::server::Context::new(
                                rpc_call.context, rx, caller_sender), rpc_call.args);
                    })
                }
                // NOCOM(#sirver): return an error - though if that has happened the
                // server messed up too.
                Ok(spinner::Command::Continue)
            },
            ::ipc::Message::RpcCancel(rpc_cancel) => {
                // NOCOM(#sirver): on drop, the rpcservercontext must delete the entry.
                if let Some(function) = self.running_rpc_calls.remove(&rpc_cancel.context) {
                    // The function might be dead already, so we ignore errors.
                    let _ = function.commands.send(rpc::server::Command::Cancel);
                }
                Ok(spinner::Command::Continue)
            },
            ::ipc::Message::RpcResponse(rpc_data) => {
                try!(self.caller_sender.send(rpc_caller::Command::OutgoingCallFinished(rpc_data)));
                Ok(spinner::Command::Continue)
            },
        }
    }
}

pub fn spawn(messages: mpsc::Receiver<::ipc::Message>, caller_sender: mpsc::Sender<rpc_caller::Command>) -> thread::JoinHandle<()> {
    let receiver = Receiver::new(messages);
    let handler = Handler::new(caller_sender);
    spinner::spawn(receiver, handler)
}
