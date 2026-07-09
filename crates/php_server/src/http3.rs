use super::php_request::RequestLocalAddr;
use crate::{
    response::ResponseBody,
    serve::{bytes_request_body, handle_parts},
    server::ServerError,
    state::AppState,
    tls::build_quic_server_config,
};
use bytes::{Buf, Bytes, BytesMut};
use h3::server::RequestStream;
use http_body_util::BodyExt;
use hyper::{
    Response, StatusCode,
    header::{self, HeaderName},
};
use std::{net::SocketAddr, path::Path, sync::Arc};
use tokio::task::JoinSet;
use tracing::{debug, warn};

pub(crate) fn build_http3_endpoint(
    cert_path: &Path,
    key_path: &Path,
    listen: SocketAddr,
) -> Result<quinn::Endpoint, ServerError> {
    let server_config = build_quic_server_config(cert_path, key_path)?;
    quinn::Endpoint::server(server_config, listen).map_err(ServerError::Io)
}

pub(crate) async fn serve_http3_endpoint(endpoint: quinn::Endpoint, state: Arc<AppState>) {
    let mut tasks = JoinSet::new();
    let local_addr = match endpoint.local_addr() {
        Ok(addr) => addr,
        Err(error) => {
            warn!(%error, "HTTP/3 endpoint local address unavailable");
            return;
        }
    };
    while let Some(incoming) = endpoint.accept().await {
        let peer = incoming.remote_address();
        let state = Arc::clone(&state);
        tasks.spawn(async move {
            match incoming.await {
                Ok(connection) => serve_http3_connection(connection, state, peer, local_addr).await,
                Err(error) => warn!(%peer, %error, "HTTP/3 QUIC handshake failed"),
            }
        });
        while let Some(result) = tasks.try_join_next() {
            if let Err(error) = result {
                warn!(%error, "HTTP/3 connection task failed");
            }
        }
    }
}

async fn serve_http3_connection(
    connection: quinn::Connection,
    state: Arc<AppState>,
    peer: SocketAddr,
    local_addr: SocketAddr,
) {
    let quic = h3_quinn::Connection::new(connection);
    let mut connection = match h3::server::builder().build(quic).await {
        Ok(connection) => connection,
        Err(error) => {
            warn!(%peer, %error, "HTTP/3 connection setup failed");
            return;
        }
    };

    loop {
        match connection.accept().await {
            Ok(Some(resolver)) => {
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    match resolver.resolve_request().await {
                        Ok((request, stream)) => {
                            handle_http3_request(request, stream, state, peer, local_addr).await
                        }
                        Err(error) => warn!(%peer, %error, "HTTP/3 request resolution failed"),
                    }
                });
            }
            Ok(None) => break,
            Err(error) => {
                debug!(%peer, %error, "HTTP/3 connection accept ended");
                break;
            }
        }
    }
}

async fn handle_http3_request(
    request: hyper::Request<()>,
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    state: Arc<AppState>,
    peer: SocketAddr,
    local_addr: SocketAddr,
) {
    let (mut parts, ()) = request.into_parts();
    parts.extensions.insert(RequestLocalAddr(local_addr));
    let body = match read_http3_request_body(&mut stream, state.max_body_bytes).await {
        Ok(body) => body,
        Err(Http3BodyReadError::Invalid(error)) => {
            warn!(%peer, %error, "HTTP/3 request body read failed");
            let mut response = Response::new(());
            *response.status_mut() = StatusCode::BAD_REQUEST;
            if let Err(error) = stream.send_response(response).await {
                warn!(%peer, %error, "HTTP/3 bad-request response failed");
            }
            let _ = stream.finish().await;
            return;
        }
    };
    let response = handle_parts(parts, bytes_request_body(body), state, peer).await;
    if let Err(error) = send_http3_response(stream, response).await {
        warn!(%peer, %error, "HTTP/3 response send failed");
    }
}

#[derive(Debug)]
enum Http3BodyReadError {
    Invalid(String),
}

async fn read_http3_request_body(
    stream: &mut RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    max_body_bytes: usize,
) -> Result<Bytes, Http3BodyReadError> {
    let mut body = BytesMut::new();
    while let Some(mut chunk) = stream
        .recv_data()
        .await
        .map_err(|error| Http3BodyReadError::Invalid(error.to_string()))?
    {
        while chunk.has_remaining() {
            let bytes = chunk.copy_to_bytes(chunk.remaining());
            body.extend_from_slice(&bytes);
            if body.len() > max_body_bytes {
                return Ok(body.freeze());
            }
        }
    }
    Ok(body.freeze())
}

async fn send_http3_response(
    mut stream: RequestStream<h3_quinn::BidiStream<Bytes>, Bytes>,
    response: Response<ResponseBody>,
) -> Result<(), String> {
    let (mut parts, body) = response.into_parts();
    strip_http3_forbidden_headers(&mut parts.headers);
    stream
        .send_response(Response::from_parts(parts, ()))
        .await
        .map_err(|error| error.to_string())?;
    let body = body
        .collect()
        .await
        .map_err(|error| error.to_string())?
        .to_bytes();
    if !body.is_empty() {
        stream
            .send_data(body)
            .await
            .map_err(|error| error.to_string())?;
    }
    stream.finish().await.map_err(|error| error.to_string())
}

fn strip_http3_forbidden_headers(headers: &mut header::HeaderMap) {
    for name in [
        header::CONNECTION,
        header::TRANSFER_ENCODING,
        header::UPGRADE,
        HeaderName::from_static("keep-alive"),
        HeaderName::from_static("proxy-connection"),
    ] {
        headers.remove(name);
    }
}
