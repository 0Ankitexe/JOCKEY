//! Exact synthetic values. Strings and domain records never grant host authority.
pub mod arena;
mod validate;
pub(crate) mod wire;
pub use validate::validate;
pub(crate) use validate::{census, consistency, type_census, types_equal, value_type};

use jocky_forensic::contracts::PossibleErrorCode;
use jocky_shared::compiler::{NamedType, Platform, Type};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Value {
    Bool {
        value: bool,
    },
    Int {
        value: String,
    },
    String {
        value: String,
    },
    Bytes {
        value: String,
    },
    Duration {
        magnitude: String,
        unit: DurationUnit,
    },
    Timestamp {
        value: String,
    },
    Endpoint {
        value: Endpoint,
    },
    Platform {
        value: Platform,
    },
    IpAddress {
        value: String,
    },
    Path {
        value: String,
    },
    ProcessRecord {
        value: ProcessRecord,
    },
    ConnectionRecord {
        value: ConnectionRecord,
    },
    FileRecord {
        value: FileRecord,
    },
    EventRecord {
        value: EventRecord,
    },
    PersistenceRecord {
        value: PersistenceRecord,
    },
    DriverRecord {
        value: DriverRecord,
    },
    ForensicResult {
        value: ForensicResult,
    },
    DiagnosticError {
        code: PossibleErrorCode,
    },
    List {
        element_type: Type,
        values: Vec<Value>,
    },
    Option {
        element_type: Type,
        value: Option<Box<Value>>,
    },
    Result {
        ok_type: Type,
        error_type: Type,
        variant: ResultVariant,
        value: Box<Value>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum DurationUnit {
    #[serde(rename = "ms")]
    Milliseconds,
    #[serde(rename = "s")]
    Seconds,
    #[serde(rename = "m")]
    Minutes,
    #[serde(rename = "h")]
    Hours,
    #[serde(rename = "d")]
    Days,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultVariant {
    Ok,
    Error,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Tcp,
    Udp,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorKind {
    FixtureMatch,
}

macro_rules! data {
    ($name:ident {$($field:ident:$ty:ty),* $(,)?}) => {
        #[derive(Clone,Debug,Deserialize,Eq,PartialEq,Serialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {$(pub $field:$ty),*}
    };
}
data!(Endpoint {
    endpoint_id: String,
    platform: Platform,
    label: String
});
data!(System {
    platform: Platform,
    hostname: String,
    release: String
});
data!(Indicator { indicator_id:String, kind:IndicatorKind, message:String, record_ids:Vec<String> });
data!(ForensicResult { endpoint_id:String, observed_at:String, system:Option<System>, indicators:Vec<Indicator> });
data!(ProcessRecord {
    record_id: String,
    endpoint_id: String,
    observed_at: String,
    pid: String,
    name: String,
    image_path: String
});
data!(ConnectionRecord { record_id:String,endpoint_id:String,observed_at:String,process_record_id:Option<String>,protocol:Protocol,local_address:String,local_port:u16,remote_address:String,remote_port:u16 });
data!(FileRecord { record_id:String,endpoint_id:String,observed_at:String,path:String,size:String,sha256:Option<String> });
data!(EventRecord {
    record_id: String,
    endpoint_id: String,
    observed_at: String,
    source: String,
    event_code: String,
    message: String
});
data!(PersistenceRecord {
    record_id: String,
    endpoint_id: String,
    observed_at: String,
    mechanism: String,
    location: String,
    description: String
});
data!(DriverRecord { record_id:String,endpoint_id:String,observed_at:String,name:String,path:String,sha256:Option<String> });

pub(crate) fn drain_type(root: &mut Type) {
    let mut stack = vec![std::mem::replace(root, Type::named(NamedType::Bool))];
    while let Some(ty) = stack.pop() {
        match ty {
            Type::List { element } | Type::Option { element } => stack.push(*element),
            Type::Result { ok, error } => {
                stack.push(*ok);
                stack.push(*error);
            }
            Type::Named { .. } => {}
        }
    }
}
pub(crate) fn drain_values(stack: &mut Vec<Value>) {
    while let Some(value) = stack.pop() {
        match value {
            Value::List {
                mut element_type,
                mut values,
            } => {
                drain_type(&mut element_type);
                stack.append(&mut values);
            }
            Value::Option {
                mut element_type,
                value,
            } => {
                drain_type(&mut element_type);
                if let Some(v) = value {
                    stack.push(*v);
                }
            }
            Value::Result {
                mut ok_type,
                mut error_type,
                value,
                ..
            } => {
                drain_type(&mut ok_type);
                drain_type(&mut error_type);
                stack.push(*value);
            }
            _ => {}
        }
    }
}
