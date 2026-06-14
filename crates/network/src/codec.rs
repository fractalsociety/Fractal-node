use std::io::{self, ErrorKind};

use async_trait::async_trait;
use borsh::BorshDeserialize;
use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use libp2p::request_response::Codec;
use libp2p::swarm::StreamProtocol;

use crate::{SyncRequest, SyncResponse};

/// Maximum encoded request/response payload (bytes after the 4-byte length prefix).
const MAX_PAYLOAD: u32 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default)]
pub struct BorshSyncCodec;

async fn read_frame<R: AsyncRead + Unpin + Send>(r: &mut R) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let n = u32::from_be_bytes(len_buf);
    if n > MAX_PAYLOAD {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("sync frame too large: {n}"),
        ));
    }
    let mut buf = vec![0u8; n as usize];
    r.read_exact(&mut buf).await?;
    Ok(buf)
}

async fn write_frame<W: AsyncWrite + Unpin + Send>(w: &mut W, payload: &[u8]) -> io::Result<()> {
    let n: u32 = payload
        .len()
        .try_into()
        .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "payload length overflow"))?;
    w.write_all(&n.to_be_bytes()).await?;
    w.write_all(payload).await?;
    w.close().await?;
    Ok(())
}

#[async_trait]
impl Codec for BorshSyncCodec {
    type Protocol = StreamProtocol;
    type Request = SyncRequest;
    type Response = SyncResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let buf = read_frame(io).await?;
        SyncRequest::try_from_slice(&buf).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let buf = read_frame(io).await?;
        SyncResponse::try_from_slice(&buf).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let payload = borsh::to_vec(&req).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
        write_frame(io, &payload).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let payload = borsh::to_vec(&res).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
        write_frame(io, &payload).await
    }
}
