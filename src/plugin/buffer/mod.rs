// Copyright (c) The Swiboe development team. All rights reserved.
// Licensed under the Apache License, Version 2.0. See LICENSE.txt
// in the project root for license information.

use ::client;
use ::plugin;
use std::sync::{RwLock, Arc};

pub struct Core;

impl plugin::Core for Core {
    fn rpc_map(&self, thin_client: client::ThinClient) -> plugin::RpcMap {
        let buffers = Arc::new(RwLock::new(base::BuffersManager::new(thin_client)));
        rpc_map! {
            "buffer.new" => new::Rpc { buffers: buffers.clone() },
            "buffer.delete" => delete::Rpc { buffers: buffers.clone() },
            "buffer.get_content" => get_content::Rpc { buffers: buffers.clone() },
            "buffer.open" => open::Rpc { buffers: buffers.clone() },
            "buffer.list" => list::Rpc { buffers: buffers.clone() },
        }
    }
}

mod base;
pub mod new;
pub mod delete;
pub mod get_content;
pub mod open;
pub mod list;
