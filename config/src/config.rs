/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements.  See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License.  You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::{any, collections::HashMap};

use super::protocol::ProtocolConfig;
use super::service::ServiceConfig;

/// used to storage all structed config, from some source: cmd, file..;
/// Impl Config trait, business init by read Config trait
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct RootConfig {
    pub name: String,
    pub service: HashMap<String, ServiceConfig>,
    pub protocols: HashMap<String, ProtocolConfig>,
    pub data: HashMap<String, Box<dyn any::Any>>,
}

pub fn get_global_config() -> RootConfig {
    let mut c = RootConfig::new();
    c.load();
    c
}

impl RootConfig {
    pub fn new() -> Self {
        Self {
            name: "dubbo".to_string(),
            service: HashMap::new(),
            protocols: HashMap::new(),
            data: HashMap::new(),
        }
    }

    pub fn load(&mut self) {
        let service_config = ServiceConfig::default()
            .group("test".to_string())
            .serializer("json".to_string())
            .version("1.0.0".to_string())
            .protocol_names("triple".to_string())
            .name("echo".to_string());

        let triple_config = ProtocolConfig::default()
            .name("triple".to_string())
            .ip("0.0.0.0".to_string())
            .port("8888".to_string());

        let service_config = service_config.add_protocol_configs(triple_config);
        self.service.insert("echo".to_string(), service_config);
        self.service.insert(
            "helloworld.Greeter".to_string(),
            ServiceConfig::default()
                .group("test".to_string())
                .serializer("json".to_string())
                .version("1.0.0".to_string())
                .name("helloworld.Greeter".to_string())
                .protocol_names("triple".to_string()),
        );
        self.protocols.insert(
            "triple".to_string(),
            ProtocolConfig::default()
                .name("triple".to_string())
                .ip("0.0.0.0".to_string())
                .port("8889".to_string()),
        );
        // 通过环境变量读取某个文件。加在到内存中
        self.data.insert(
            "dubbo.provider.url".to_string(),
            Box::new("dubbo://127.0.0.1:8888/?serviceName=hellworld".to_string()),
        );
        // self.data.insert("dubbo.consume.", v)
    }
}

impl Config for RootConfig {
    fn bool(&self, key: String) -> bool {
        match self.data.get(&key) {
            None => false,
            Some(val) => {
                if let Some(v) = val.downcast_ref::<bool>() {
                    *v
                } else {
                    false
                }
            }
        }
    }

    fn string(&self, key: String) -> String {
        match self.data.get(&key) {
            None => "".to_string(),
            Some(val) => {
                if let Some(v) = val.downcast_ref::<String>() {
                    v.into()
                } else {
                    "".to_string()
                }
            }
        }
    }
}

pub trait BusinessConfig {
    fn init() -> Self;
    fn load() -> Result<(), std::convert::Infallible>;
}

pub trait Config {
    fn bool(&self, key: String) -> bool;
    fn string(&self, key: String) -> String;
}
