use napi_derive::napi;

#[napi]
pub fn health_check() -> String {
    "rw-napi is working".to_owned()
}
