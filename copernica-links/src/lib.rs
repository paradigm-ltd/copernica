mod udpipv4;
mod mpsc_channel;
mod mpsc_corruptor;
pub use {
    udpipv4::{UdpIpV4},
    mpsc_channel::{MpscChannel},
    mpsc_corruptor::{MpscCorruptor, Corruption},
};
use {
    copernica_packets::{
        InterLinkPacket, LinkId, LinkPacket, PublicIdentity,
    },
    copernica_common::{ Operations },
    crossbeam_channel::{Receiver, Sender},
    anyhow::{anyhow, Result},
    reed_solomon::{Buffer, Encoder, Decoder},
};
pub fn decode(msg: Vec<u8>, link_id: LinkId) -> Result<(PublicIdentity, LinkPacket)> {
    let dec = Decoder::new(6);
    let mut buffers: Vec<Buffer> = vec![];
    for chunk in msg.chunks(255) {
        buffers.push(Buffer::from_slice(chunk, chunk.len()));
    }
    let mut reconstituted: Vec<Buffer> = vec![];
    for buffer in buffers {
        let buf = match dec.correct(&buffer, None) {
            Ok(b) => b,
            Err(e) => {
                return Err(anyhow!("Packet corrupted beyond recovery, dropping it (error: {:?})", e));
            },
        };
        reconstituted.push(buf);
    }
    let reconstituted: Vec<u8> = reconstituted.iter().map(|d| d.data()).collect::<Vec<_>>().concat();
    let (public_id0, lp0) = LinkPacket::from_bytes(&reconstituted, link_id.clone())?;
    Ok((public_id0, lp0))
}
pub fn encode(lp: LinkPacket, link_id: LinkId) -> Result<Vec<u8>> {
    let mut merged = vec![];
    let enc = Encoder::new(6);
    let lpb: Vec<u8> = lp.as_bytes(link_id.clone())?;
    let cs = lpb.chunks(255-6);
    for c in cs {
        let c = enc.encode(&c[..]);
        merged.extend(&**c);
    }
    Ok(merged)
}
pub trait Link {
    fn run(&mut self) -> Result<()>;
    fn new(link: LinkId, ops: (String, Operations), router_in_and_out: ( Sender<InterLinkPacket> , Receiver<InterLinkPacket>)) -> Result<Self> where Self: Sized;
}
