use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct LabelRef<'a> {
    name: &'a str,
    value: &'a str,
}

#[derive(Serialize)]
pub(crate) struct Controller<'a> {
    pub(crate) type_: &'a str,
    pub(crate) name: &'a str,
}
