use bytes::Bytes;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::{Response, StatusCode, header};
use std::convert::Infallible;

pub type ResponseBody = BoxBody<Bytes, Infallible>;

pub fn text(status: StatusCode, body: &'static str) -> Response<ResponseBody> {
    response(status, Bytes::from_static(body.as_bytes()))
}

pub fn text_dynamic(
    status: StatusCode,
    body: String,
    content_type: &'static str,
) -> Response<ResponseBody> {
    bytes(status, Bytes::from(body), content_type)
}

pub fn empty(status: StatusCode) -> Response<ResponseBody> {
    response(status, Bytes::new())
}

pub fn bytes(
    status: StatusCode,
    body: Bytes,
    content_type: &'static str,
) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_LENGTH, body.len().to_string())
        .header(header::CONTENT_TYPE, content_type)
        .body(Full::new(body).boxed())
        .expect("static response builder is valid")
}

pub fn static_head(
    status: StatusCode,
    content_length: u64,
    content_type: &'static str,
) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_LENGTH, content_length.to_string())
        .header(header::CONTENT_TYPE, content_type)
        .body(Full::new(Bytes::new()).boxed())
        .expect("static response builder is valid")
}

pub fn response(status: StatusCode, body: Bytes) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .body(Full::new(body).boxed())
        .expect("static response builder is valid")
}
