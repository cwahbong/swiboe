// Copyright (c) The Swiboe development team. All rights reserved.
// Licensed under the Apache License, Version 2.0. See LICENSE.txt
// in the project root for license information.

#![allow(deprecated)]

use ::client::conn::{Connection, TcpConnector, UnixConnector};
use ::error::Result;
use ::ipc;

// NOCOM such class/module should be pulled out
//       server and client should not depend each other
use ::server::plugin_core::NewRpcRequest;

use serde;
use std::io;
use std::net;
use std::path;
use std::sync::{mpsc, Mutex};
use std::thread;

/// An abstraction that can call remove RPCs.
pub trait RpcCaller {
    fn call<T: serde::Serialize>(&mut self, function: &str, args: &T) -> Result<::client::rpc::client::Context>;
}

/// A client maintains a connection to a Swiboe server. It can also serve RPCs that can only be
/// called by the server.
pub struct Client {
    // Connection to the logic loop.
    rpc_loop_commands: rpc_loop::CommandSender,

    // The thread dealing with all the logic in the client.
    rpc_loop_thread: Option<thread::JoinHandle<()>>,

    // The threads dealing with IO. There is a separate thread for reading and one for writing.
    // Both of them block on their IO.
    reader_thread: Option<thread::JoinHandle<()>>,
    writer_thread: Option<thread::JoinHandle<()>>,

    // Function to bring down the connection used for IO. The 'read_thread' and 'write_thread' will
    // both error then and terminate.
    shutdown_func: Box<Fn() -> ()>,
}


impl Client {
    pub fn connect_unix(socket_name: &path::Path) -> Result<Self> {
        Client::start(UnixConnector::new(socket_name))
    }

    pub fn connect_tcp(address: &net::SocketAddr) -> Result<Self> {
        Client::start(TcpConnector::new(address))
    }

    fn spawn_reader<R: io::Read + Send + 'static>(read: R, commands_tx: mpsc::Sender<rpc_loop::Command>) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut reader = ipc::Reader::new(read);
            while let Ok(message) = reader.read_message() {
                let command = rpc_loop::Command::Received(message);
                if commands_tx.send(command).is_err() {
                    break;
                }
            };
        })
    }

    fn spawn_writer<W: io::Write + Send + 'static>(write: W, send_rx: mpsc::Receiver<ipc::Message>) -> thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut writer = ipc::Writer::new(write);
            while let Ok(message) = send_rx.recv() {
                writer.write_message(&message).expect("Writing failed");
            }
        })
    }

    pub fn start<C: ::client::conn::Connector>(connector: C) -> Result<Self> {
        let (commands_tx, commands_rx) = mpsc::channel();
        let (send_tx, send_rx) = mpsc::channel();
        let mut connection = try!(connector.connect());
        Ok(Client {
            rpc_loop_commands: commands_tx.clone(),
            rpc_loop_thread: Some(rpc_loop::spawn(commands_rx, commands_tx.clone(), send_tx)),
            reader_thread: Some(Self::spawn_reader(try!(connection.reader()), commands_tx)),
            writer_thread: Some(Self::spawn_writer(try!(connection.writer()), send_rx)),
            shutdown_func: try!(connection.shutdown_func()),
        })
    }

    pub fn new_rpc(&mut self, name: &str, rpc: Box<rpc::server::Rpc>) -> Result<()> {
        let mut new_rpc = try!(self.call("core.new_rpc", &NewRpcRequest {
            priority: rpc.priority(),
            name: name.into(),
        }));
        let result = new_rpc.wait();

        if !result.is_ok() {
            return Err(result.unwrap_err().into());
        }

        self.rpc_loop_commands.send(rpc_loop::Command::NewRpc(name.into(), rpc)).expect("NewRpc");
        Ok(())
    }

    pub fn clone(&self) -> Result<ThinClient> {
        Ok(ThinClient {
            rpc_loop_commands: Mutex::new(self.rpc_loop_commands.clone()),
        })
    }
}

impl RpcCaller for Client {
    fn call<T: serde::Serialize>(&mut self, function: &str, args: &T) -> Result<rpc::client::Context> {
        rpc::client::Context::new(self.rpc_loop_commands.clone(), function, args)
    }
}


impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.rpc_loop_commands.send(rpc_loop::Command::Quit);
        if let Some(thread) = self.rpc_loop_thread.take() {
            thread.join().expect("Joining rpc_loop_thread failed.");
        }

        (self.shutdown_func)();

        if let Some(thread) = self.writer_thread.take() {
            thread.join().expect("Joining writer_thread failed.");
        }
        if let Some(thread) = self.reader_thread.take() {
            thread.join().expect("Joining reader_thread failed.");
        }
    }
}

/// A ThinClient is an RpcCaller, but does not maintain and cannot register new RPCs. It can
/// be cloned, so that many threads can do RPCs in parallel.
pub struct ThinClient {
    rpc_loop_commands: Mutex<rpc_loop::CommandSender>,
}

impl ThinClient {
    pub fn clone(&self) -> Self {
        let commands = {
            let commands = self.rpc_loop_commands.lock().unwrap();
            commands.clone()
        };
        ThinClient {
            rpc_loop_commands: Mutex::new(commands),
        }
    }
}

impl RpcCaller for ThinClient {
    fn call<T: serde::Serialize>(&mut self, function: &str, args: &T) -> Result<rpc::client::Context> {
        let commands = {
            let commands = self.rpc_loop_commands.lock().unwrap();
            commands.clone()
        };
        rpc::client::Context::new(commands, function, args)
    }
}


mod rpc_loop;

pub mod conn;
pub mod rpc;
