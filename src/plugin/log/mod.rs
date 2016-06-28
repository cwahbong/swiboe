// Copyright (c) The Swiboe development team. All rights reserved.
// Licensed under the Apache License, Version 2.0. See LICENSE.txt
// in the project root for license information.
use ::client;
use ::plugin;
use time;


pub struct Core;

impl plugin::Core for Core {
    fn rpc_map(&self, _thin_client: client::ThinClient) -> plugin::RpcMap {
        rpc_map! {
            "log.debug" => debug::Rpc,
            "log.info" => info::Rpc,
            "log.warn" => warn::Rpc,
            "log.error" => error::Rpc,
        }
    }
}

pub fn current() -> String {
    format!("{}", time::now_utc().rfc3339())
}

mod base;
pub mod debug;
pub mod info;
pub mod warn;
pub mod error;
