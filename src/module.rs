use std::collections::BTreeMap;

use crate::{NamedArguments, Result, Value};

#[derive(Default)]
pub struct Module {
    values: BTreeMap<String, Value>,
}

impl Module {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: impl Into<String>, value: Value) -> &mut Self {
        self.values.insert(name.into(), value);
        self
    }

    pub fn register_fn(
        &mut self,
        name: impl Into<String>,
        function: impl Fn(Vec<Value>, NamedArguments) -> Result<Value> + 'static,
    ) -> &mut Self {
        self.set(name, Value::native(function))
    }
}

impl From<Module> for Value {
    fn from(module: Module) -> Self {
        Self::Object(module.values)
    }
}
