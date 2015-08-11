#![allow(deprecated)]

use mio::unix::UnixStream;
use mio;
use serde::{json, Serialize, Deserialize};
use std::collections::HashMap;
use std::path;
use std::sync::mpsc;
use std::thread;
use super::Result;
use super::ipc;
use super::plugin_core::NewRpcRequest;
use uuid::Uuid;

const CLIENT: mio::Token = mio::Token(1);

pub trait RemoteProcedure: Send {
    fn priority(&self) -> u16 { u16::max_value() }
    fn call(&mut self, args: json::Value) -> ipc::RpcResult;
}

pub struct Rpc {
    // NOCOM(#sirver): something more structured?
    pub values: mpsc::Receiver<ipc::RpcResponse>,
}

impl Rpc {
    fn new(values: mpsc::Receiver<ipc::RpcResponse>) -> Self {
        // NOCOM(#sirver): implement drop so that we can cancel an RPC.
        Rpc {
            values: values,
        }
    }

    // NOCOM(#sirver): timeout?
    fn recv(&self) -> Result<ipc::RpcResponse> {
        Ok(try!(self.values.recv()))
    }

    // NOCOM(#sirver): kill?
    pub fn wait(self) -> Result<ipc::RpcResult> {
        // NOCOM(#sirver): how does streaming work?
        let rpc_response = try!(self.recv());
        Ok(rpc_response.result)
    }

    // NOCOM(#sirver): figure out error handling for clients, not use Server error?
    pub fn wait_for<T: Deserialize>(&self) -> Result<T> {
        let rpc_response = try!(self.recv());
        match rpc_response.result {
            ipc::RpcResult::Ok(value) => Ok(try!(json::from_value(value))),
            ipc::RpcResult::Err(err) => panic!("#sirver err: {:#?}", err),
            // NOCOM(#sirver): probably should ignore other errors.
            other => panic!("#sirver other: {:#?}", other),
        }
    }
}

pub enum EventLoopThreadCommand {
    Quit,
    Send(ipc::Message),
    Call(String, mpsc::Sender<ipc::RpcResponse>),
}

// NOCOM(#sirver): bad name
struct Handler<'a> {
    stream: ipc::IpcStream<UnixStream>,
    running_function_calls: HashMap<String, mpsc::Sender<ipc::RpcResponse>>,
    function_thread_sender: mpsc::Sender<FunctionThreadCommand<'a>>,
}

impl<'a> mio::Handler for Handler<'a> {
    type Timeout = ();
    type Message = EventLoopThreadCommand;

    fn notify(&mut self, event_loop: &mut mio::EventLoop<Self>, command: EventLoopThreadCommand) {
        match command {
            EventLoopThreadCommand::Quit => event_loop.shutdown(),
            EventLoopThreadCommand::Send(message) => {
                if let Err(err) = self.stream.write_message(&message) {
                    println!("Shutting down, since sending failed: {:?}", err);
                    event_loop.channel().send(EventLoopThreadCommand::Quit).expect("Quit");
                }
            },
            EventLoopThreadCommand::Call(context, tx) => {
                self.running_function_calls.insert(context, tx);
            },
        }
    }

    fn ready(&mut self, event_loop: &mut mio::EventLoop<Self>, token: mio::Token, events: mio::EventSet) {
        match token {
            CLIENT => {
                if events.is_hup() {
                    event_loop.channel().send(EventLoopThreadCommand::Quit).expect("EventLoopThreadCommand::Quit");
                    return;
                }

                if events.is_readable() {
                    loop {
                        let message;
                        match self.stream.read_message() {
                            Err(err) => {
                                println!("Shutting down, since receiving failed: {:?}", err);
                                event_loop.channel().send(EventLoopThreadCommand::Quit).expect("EventLoopThreadCommand::Quit");
                                return;
                            }
                            Ok(None) => break,
                            Ok(Some(msg)) => message = msg,
                        };

                        match message {
                            ipc::Message::RpcResponse(rpc_data) => {
                                // This will quietly drop any updates on functions that we no longer
                                // know/care about.
                                self.running_function_calls
                                    .get(&rpc_data.context)
                                    .map(|channel| {
                                        // The other side of this channel might not exist anymore - we
                                        // might have dropped the RPC already. Just ignore it.
                                        let _ = channel.send(rpc_data);
                                    });
                            },
                            ipc::Message::RpcCall(rpc_call) => {
                                let command = FunctionThreadCommand::Call(rpc_call);
                                self.function_thread_sender.send(command).expect("FunctionThreadCommand::Call");
                            },
                        }
                    }
                    event_loop.reregister(
                        &self.stream.socket,
                        CLIENT,
                        mio::EventSet::readable(),
                        mio::PollOpt::edge() | mio::PollOpt::oneshot()).unwrap();
                }
            },
            client_token => panic!("Unexpected token: {:?}", client_token),
        }
    }
}

enum FunctionThreadCommand<'a> {
    Quit,
    NewRpc(String, Box<RemoteProcedure + 'a>),
    Call(ipc::RpcCall),
}

struct FunctionThread<'a> {
    remote_procedures: HashMap<String, Box<RemoteProcedure + 'a>>,
    commands: mpsc::Receiver<FunctionThreadCommand<'a>>,
    event_loop_sender: mio::Sender<EventLoopThreadCommand>,
}

impl<'a> FunctionThread<'a> {
    fn spin_forever(&mut self) {
        while let Ok(command) = self.commands.recv() {
            match command {
                FunctionThreadCommand::Quit => break,
                FunctionThreadCommand::NewRpc(name, remote_procedures) => {
                    self.remote_procedures.insert(name, remote_procedures);
                },
                FunctionThreadCommand::Call(rpc_call) => {
                    if let Some(function) = self.remote_procedures.get_mut(&rpc_call.function) {
                        let result = function.call(rpc_call.args);

                        // Ignore error on send: if the event_loop is no longer listening, somebody
                        // will send us a Quit command soon enough too.
                        let _ = self.event_loop_sender.send(EventLoopThreadCommand::Send(
                            ipc::Message::RpcResponse(ipc::RpcResponse {
                                context: rpc_call.context,
                                // NOCOM(#sirver): what about streaming rpcs?
                                result: result,
                            })));
                    }
                    // NOCOM(#sirver): return an error - though if that has happened the
                    // server messed up too.

                    // NOCOM(#sirver): implement
                }
            }
        }
    }
}

// NOCOM(#sirver): Return a future? How about streaming functions?
fn call<T: Serialize>(event_loop_sender: &mio::Sender<EventLoopThreadCommand>, function: &str, args: &T) -> Rpc {
    let args = json::to_value(&args);
    let context = Uuid::new_v4().to_hyphenated_string();
    let message = ipc::Message::RpcCall(ipc::RpcCall {
        function: function.into(),
        context: context.clone(),
        args: args,
    });

    let (tx, rx) = mpsc::channel();
    event_loop_sender.send(EventLoopThreadCommand::Call(context, tx)).expect("Call");
    event_loop_sender.send(EventLoopThreadCommand::Send(message)).expect("Send");
    Rpc::new(rx)
}

pub struct Client<'a> {
    event_loop_sender: mio::Sender<EventLoopThreadCommand>,
    _function_thread_join_guard: thread::JoinGuard<'a, ()>,

    function_thread_sender: mpsc::Sender<FunctionThreadCommand<'a>>,
}

impl<'a> Client<'a> {
    pub fn connect(socket_name: &path::Path) -> Self {
        let stream = UnixStream::connect(&socket_name).unwrap();

        let mut event_loop = mio::EventLoop::<Handler>::new().unwrap();
        let event_loop_sender = event_loop.channel();

        let (commands_tx, commands_rx) = mpsc::channel();
        let mut handler = Handler {
            stream: ipc::IpcStream::new(stream),
            running_function_calls: HashMap::new(),
            function_thread_sender: commands_tx.clone(),
        };
        event_loop.register_opt(
            &handler.stream.socket, CLIENT, mio::EventSet::readable(), mio::PollOpt::edge() |
            mio::PollOpt::oneshot()).unwrap();
        let event_loop_thread = thread::scoped(move || {
            event_loop.run(&mut handler).unwrap();
        });

        Client {
            event_loop_sender: event_loop_sender.clone(),
            function_thread_sender: commands_tx,
            _function_thread_join_guard: thread::scoped(move || {
                let mut thread = FunctionThread {
                    remote_procedures: HashMap::new(),
                    commands: commands_rx,
                    event_loop_sender: event_loop_sender,
                };
                thread.spin_forever();

                // If the remote side has disconnected us, the event_loop will already be destroyed and
                // this send will fail.
                let _ = thread.event_loop_sender.send(EventLoopThreadCommand::Quit);
                event_loop_thread.join();
            }),
        }
    }

    // NOCOM(#hrapp): 'a needed?
    pub fn new_rpc(&self, name: &str, remote_procedure: Box<RemoteProcedure + 'a>) {
        // NOCOM(#sirver): what happens when this is already inserted? crash probably
        let rpc = self.call("core.new_rpc", &NewRpcRequest {
            priority: remote_procedure.priority(),
            name: name.into(),
        });
        let success = rpc.wait().unwrap();
        // NOCOM(#sirver): report failure.

        // NOCOM(#hrapp): bring back
        self.function_thread_sender.send(FunctionThreadCommand::NewRpc(
                name.into(), remote_procedure)).expect("NewRpc");
    }

    pub fn call<T: Serialize>(&self, function: &str, args: &T) -> Rpc {
        call(&self.event_loop_sender, function, args)
    }

    pub fn new_rpc_caller(&self) -> RpcCaller {
        RpcCaller {
            event_loop_sender: self.event_loop_sender.clone(),
        }
    }
}

impl<'a> Drop for Client<'a> {
    fn drop(&mut self) {
        self.function_thread_sender.send(FunctionThreadCommand::Quit).unwrap();
    }
}

pub struct RpcCaller {
    event_loop_sender: mio::Sender<EventLoopThreadCommand>,
}

impl RpcCaller {
    pub fn call<T: Serialize>(&self, function: &str, args: &T) -> Rpc {
        call(&self.event_loop_sender, function, args)
    }
}
