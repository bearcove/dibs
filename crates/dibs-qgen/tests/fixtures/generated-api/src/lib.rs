#![forbid(unsafe_code)]

pub struct WrappedId(pub i64);

impl std::ops::Deref for WrappedId {
    type Target = i64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub mod many {
    include!(concat!(env!("OUT_DIR"), "/many.rs"));
}

pub mod optional {
    include!(concat!(env!("OUT_DIR"), "/optional.rs"));
}

pub mod one {
    include!(concat!(env!("OUT_DIR"), "/one.rs"));
}

pub mod exec {
    include!(concat!(env!("OUT_DIR"), "/exec.rs"));
}

pub fn assert_compiled() {
    let _ = many::find_widgets::<dibs_runtime::tokio_postgres::Client>;
    let _ = optional::find_optional_widget::<dibs_runtime::tokio_postgres::Client>;
    let _ = one::find_required_widget::<dibs_runtime::tokio_postgres::Client>;
    let _ = exec::delete_widget::<dibs_runtime::tokio_postgres::Client>;
}
