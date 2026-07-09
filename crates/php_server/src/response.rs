use bytes::Bytes;
use futures_util::{TryStreamExt, stream};
use http_body_util::{BodyExt, Full, StreamBody, combinators::BoxBody};
use hyper::{Response, StatusCode, body::Frame, header};
use std::convert::Infallible;
use tokio::io::AsyncRead;
use tokio_util::io::ReaderStream;

pub type ResponseBody = BoxBody<Bytes, std::io::Error>;
pub type RequestBody = BoxBody<Bytes, std::io::Error>;

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
        .body(full_body(body))
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
        .body(full_body(Bytes::new()))
        .expect("static response builder is valid")
}

pub fn response(status: StatusCode, body: Bytes) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .body(full_body(body))
        .expect("static response builder is valid")
}

pub fn full_body(body: Bytes) -> ResponseBody {
    Full::new(body)
        .map_err(|never: Infallible| match never {})
        .boxed()
}

pub fn request_body_from_bytes(body: Bytes) -> RequestBody {
    Full::new(body)
        .map_err(|never: Infallible| match never {})
        .boxed()
}

pub fn stream_body_from_bytes(body: Bytes) -> ResponseBody {
    let stream = stream::once(async move { Ok::<_, std::io::Error>(Frame::data(body)) });
    StreamBody::new(stream).boxed()
}

pub fn reader_body<R>(reader: R) -> ResponseBody
where
    R: AsyncRead + Send + Sync + 'static,
{
    let stream = ReaderStream::new(reader).map_ok(Frame::data);
    StreamBody::new(stream).boxed()
}
