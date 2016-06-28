// Copyright (c) The Swiboe development team. All rights reserved.
// Licensed under the Apache License, Version 2.0. See LICENSE.txt
// in the project root for license information.
use ::error::Result;
use ::client;
use std::collections::HashMap;

pub type RpcMap = HashMap<String, Box<client::rpc::server::Rpc>>;

macro_rules! rpc_map {
    ($($s:expr => $rpc:expr),*) => {{
        let mut map = $crate::plugin::RpcMap::new();
        $(map.insert(String::from($s), Box::new($rpc));)*
        map
    }};
    ($($s:expr => $rpc:expr),*,) => {
        rpc_map!($($s => $rpc), *)
    };
}

pub fn register_rpc(client: &mut client::Client, map: RpcMap) -> Result<()> {
    for (name, rpc) in map {
        try!(client.new_rpc(&name, rpc));
    }
    Ok(())
}

pub trait Core {
    fn rpc_map(&self, thin_client: client::ThinClient) -> RpcMap;
}

pub struct Plugin {
    _client: client::Client,
}

impl Plugin {
    pub fn start<Conn: client::conn::Connector, Co: Core>(connector: &Conn, core: Co) -> Result<Self> {
        let mut client = try!(client::Client::start(connector));
        let thin_client = try!(client.clone());
        try!(register_rpc(&mut client, core.rpc_map(thin_client)));
        Ok(Plugin { _client: client })
    }
}

pub mod buffer;
pub mod list_files;
pub mod log;
