// Copyright (c) The Swiboe development team. All rights reserved.
// Licensed under the Apache License, Version 2.0. See LICENSE.txt
// in the project root for license information.

use ::error::Result;
use std::net::{self, TcpStream};
use std::io;
use std::path;
use unix_socket::UnixStream;


pub trait Connection {
    type Read: io::Read + Send + 'static;
    type Write: io::Write + Send + 'static;

    fn reader(&mut self) -> Result<Self::Read>;
    fn writer(&mut self) -> Result<Self::Write>;
    fn shutdown_func(&mut self) -> Result<Box<Fn() -> ()>>;
}

pub trait Connector {
    type Connection: Connection;

    fn connect(&self) -> Result<Self::Connection>;
}


pub struct UnixConnection {
    stream: UnixStream,
}

impl UnixConnection {
    pub fn connect(socket_name: &path::Path) -> Result<Self> {
        Ok(UnixConnection {
            stream: try!(UnixStream::connect(socket_name)),
        })
    }
}

impl Connection for UnixConnection {
    type Read = UnixStream;
    type Write = UnixStream;

    fn reader(&mut self) -> Result<UnixStream> {
        Ok(try!(self.stream.try_clone()))
    }

    fn writer(&mut self) -> Result<UnixStream> {
        Ok(try!(self.stream.try_clone()))
    }

    fn shutdown_func(&mut self) -> Result<Box<Fn() -> ()>> {
        let shutdown_stream = try!(self.stream.try_clone());
        Ok(Box::new(move || {
            let _ = shutdown_stream.shutdown(net::Shutdown::Read);
        }))
    }
}

pub struct UnixConnector<'a> {
    socket_name: &'a path::Path,
}

impl <'a> UnixConnector<'a> {
    pub fn new(socket_name: &'a path::Path) -> Self {
        UnixConnector {
            socket_name: socket_name,
        }
    }
}

impl <'a> Connector for UnixConnector<'a> {
    type Connection = UnixConnection;

    fn connect(&self) -> Result<UnixConnection> {
        UnixConnection::connect(self.socket_name)
    }
}

pub struct TcpConnection {
    stream: TcpStream,
}

impl TcpConnection {
    pub fn connect(address: &net::SocketAddr) -> Result<Self> {
        Ok(TcpConnection {
            stream: try!(TcpStream::connect(address)),
        })
    }
}

impl Connection for TcpConnection {
    type Read = TcpStream;
    type Write = TcpStream;

    fn reader(&mut self) -> Result<Self::Read> {
        Ok(try!(self.stream.try_clone()))
    }

    fn writer(&mut self) -> Result<Self::Write> {
        Ok(try!(self.stream.try_clone()))
    }

    fn shutdown_func(&mut self) -> Result<Box<Fn() -> ()>> {
        let shutdown_stream = try!(self.stream.try_clone());
        Ok(Box::new(move || {
            let _ = shutdown_stream.shutdown(net::Shutdown::Read);
        }))
    }
}

pub struct TcpConnector<'a> {
    address: &'a net::SocketAddr,
}

impl <'a> TcpConnector<'a> {
    pub fn new(address: &'a net::SocketAddr) -> Self {
        TcpConnector {
            address: address,
        }
    }
}

impl <'a> Connector for TcpConnector<'a> {
    type Connection = TcpConnection;

    fn connect(&self) -> Result<Self::Connection> {
        Self::Connection::connect(self.address)
    }
}
