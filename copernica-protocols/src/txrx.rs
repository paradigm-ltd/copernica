use {
    copernica_common::{LinkId, NarrowWaistPacket, LinkPacket, InterLinkPacket, HBFI, PrivateIdentityInterface,
    constants, Nonce},
    log::{debug, error},
    futures::{
        stream::{StreamExt},
        channel::mpsc::{Sender, Receiver, channel},
        sink::{SinkExt},
        lock::Mutex,
    },
    smol_timeout::TimeoutExt,
    anyhow::{Result},
    std::{
        time::{Instant, Duration},
        sync::{Arc},
    },
    uluru::LRUCache,
};

/*
     s = Protocol, l = Link, b = Broker, r = Router, 2 = to: e.g. l2b = "link to copernica_broker"
     link::{udp, mpsc_channel, mpsc_corruptor, etc}
                                                            +----------------------------+
    +-----------+p2l_tx   p2l_rx+-----------+l2b_tx   l2b_rx| b2r_tx           b2r_rx    |   +-----------+   +-----------+
    |           +-------------->+           +-------------->-------------------------+   +-->+           +-->+           |
    | Protocol   |l2p_rx   l2p_tx|   Link    |b2l_rx   b2l_tx| r2b_rx       r2b_tx    |   |   |   Link    |   | Protocol   |
    |           +<--------------+           +<---------------<-------------------+   |   +<--+           +<--+           |
    +-----------+               +-----------+               |                    |   v   |   +-----------+   +-----------+
                                                            |                +---+---+-+ |
    +-----------+p2l_tx   p2l_rx+-----------+l2b_tx   l2b_rx| b2r_tx   b2r_rx|         | |   +-----------+   +-----------+
    |           +-------------->+           +-------------->---------------->+         | +-->+           +-->+           |
    | Protocol   |l2p_rx   l2p_tx|   Link    |b2l_rx   b2l_tx| r2b_rx   r2b_tx|  Router | |   |   Link    |   |  Broker   |
    |           +<--------------+           +<---------------<---------------+         | +<--+           +<--+           |
    +-----------+               +-----------+               |                |         | |   +-----------+   +-----------+
                                                            |                +---+---+-+ |
    +-----------+b2l_tx   b2l_rx+-----------+l2b_tx   l2b_rx| b2r_tx      b2r_rx ^   |   |   +-----------+   +-----------+
    |           +-------------->+           +-------------->---------------------+   |   +-->+           +-->+           |
    |  Broker   |l2b_rx   l2b_tx|   Link    |b2l_rx   b2l_tx| r2b_rx          r2b_tx |   |   |   Link    |   | Protocol   |
    |           +<--------------+           +<---------------<-----------------------+   +<--+           +<--+           |
    +-----------+               +-----------+               |           Broker           |   +-----------+   +-----------+
                                                            +----------------------------+
*/
pub type Names = Arc<Mutex<LRUCache<(Nonce, Instant, Duration), { constants::CONGESTION_CONTROL_SIZE }>>>;
#[derive(Clone)]
pub struct CongestionControl(Names);
impl CongestionControl {
    fn new() -> Self {
        Self(Names::default())
    }
    async fn start_timer(&mut self, nw: NarrowWaistPacket) {
        let names_mutex = self.0.clone();
        let mut names_ref = names_mutex.lock().await;
        let nonce = match nw {
            NarrowWaistPacket::Request { nonce, .. } => nonce,
            NarrowWaistPacket::Response{ nonce, .. } => nonce,
        };
        match names_ref.touch(|n|n.0==nonce) {
            true  => {
                let now = Instant::now();
                if let Some(front) = names_ref.front_mut() {
                    front.1 = now;
                }
            },
            false => {
                let now = Instant::now();
                names_ref.insert((nonce, now, Duration::new(1,0)));
            }
        }
    }
    async fn wait(&self, nw: NarrowWaistPacket, rx_mutex: Arc<Mutex<Receiver<InterLinkPacket>>>) -> Option<InterLinkPacket> {
        let names_mutex = self.0.clone();
        let mut names_ref = names_mutex.lock().await;
        let nonce = match nw {
            NarrowWaistPacket::Request { nonce, .. } => nonce,
            NarrowWaistPacket::Response{ nonce, .. } => nonce,
        };
        let ilp = async {
            let mut rx_ref = rx_mutex.lock().await;
            rx_ref.next().await
        };
        let res = names_ref.find(|n|n.0==nonce);
        match res {
            Some(res) => {
                let ilp = ilp.timeout(res.2);
                match ilp.await {
                    Some(Some(ilp)) => {
                        let elapsed = res.1.elapsed();
                        res.2 = elapsed;
                        return Some(ilp)
                    },
                    _ => {
                        res.2 = res.2 * 2;
                        return None
                    }
                }
            },
            None => {
                // what happens when elements are removed from the LRU??? bug?
                return None
            }
        }
    }
}
#[derive(Clone)]
pub struct TxRx {
    pub link_id: LinkId,
    pub protocol_sid: PrivateIdentityInterface,
    pub p2l_tx: Sender<InterLinkPacket>,
    pub l2p_rx: Arc<Mutex<Receiver<InterLinkPacket>>>,
    pub cc: CongestionControl,
    pub unreliable_unordered_response_tx: Sender<InterLinkPacket>,
    pub unreliable_unordered_response_rx: Arc<Mutex<Receiver<InterLinkPacket>>>,
    pub unreliable_sequenced_response_tx: Sender<InterLinkPacket>,
    pub unreliable_sequenced_response_rx: Arc<Mutex<Receiver<InterLinkPacket>>>,
    pub reliable_unordered_response_tx: Sender<InterLinkPacket>,
    pub reliable_unordered_response_rx: Arc<Mutex<Receiver<InterLinkPacket>>>,
    pub reliable_ordered_response_tx: Sender<InterLinkPacket>,
    pub reliable_ordered_response_rx: Arc<Mutex<Receiver<InterLinkPacket>>>,
    pub reliable_sequenced_response_tx: Sender<InterLinkPacket>,
    pub reliable_sequenced_response_rx: Arc<Mutex<Receiver<InterLinkPacket>>>,
}
impl TxRx {
    pub fn new(link_id: LinkId, protocol_sid: PrivateIdentityInterface, p2l_tx: Sender<InterLinkPacket>, l2p_rx: Receiver<InterLinkPacket>) -> TxRx
    {
        let (unreliable_unordered_response_tx, unreliable_unordered_response_rx) = channel::<InterLinkPacket>(constants::BOUNDED_BUFFER_SIZE);
        let (unreliable_sequenced_response_tx, unreliable_sequenced_response_rx) = channel::<InterLinkPacket>(constants::BOUNDED_BUFFER_SIZE);
        let (reliable_unordered_response_tx, reliable_unordered_response_rx) = channel::<InterLinkPacket>(constants::BOUNDED_BUFFER_SIZE);
        let (reliable_ordered_response_tx, reliable_ordered_response_rx) = channel::<InterLinkPacket>(constants::BOUNDED_BUFFER_SIZE);
        let (reliable_sequenced_response_tx, reliable_sequenced_response_rx) = channel::<InterLinkPacket>(constants::BOUNDED_BUFFER_SIZE);
        TxRx {
            link_id,
            protocol_sid,
            p2l_tx,
            l2p_rx: Arc::new(Mutex::new(l2p_rx)),
            cc: CongestionControl::new(),
            unreliable_unordered_response_rx: Arc::new(Mutex::new(unreliable_unordered_response_rx)),
            unreliable_unordered_response_tx,
            unreliable_sequenced_response_rx: Arc::new(Mutex::new(unreliable_sequenced_response_rx)),
            unreliable_sequenced_response_tx,
            reliable_unordered_response_rx: Arc::new(Mutex::new(reliable_unordered_response_rx)),
            reliable_unordered_response_tx,
            reliable_ordered_response_rx: Arc::new(Mutex::new(reliable_ordered_response_rx)),
            reliable_ordered_response_tx,
            reliable_sequenced_response_rx: Arc::new(Mutex::new(reliable_sequenced_response_rx)),
            reliable_sequenced_response_tx,
         }
    }
    pub async fn next_inbound(self) -> Option<InterLinkPacket> {
        let l2p_rx_mutex = Arc::clone(&self.l2p_rx);
        let mut l2p_rx_ref = l2p_rx_mutex.lock().await;
        l2p_rx_ref.next().await
    }
    pub async fn unreliable_unordered_request(&mut self, hbfi: HBFI, start: u64, end: u64) -> Result<Vec<Vec<u8>>> {
        let mut counter = start;
        let mut reconstruct: Vec<Vec<u8>> = vec![];
        while counter <= end {
            let hbfi_req = hbfi.clone().offset(counter);
            let nw = NarrowWaistPacket::request(hbfi_req)?;
            let lp = LinkPacket::new(self.link_id.reply_to()?, nw.clone());
            let ilp = InterLinkPacket::new(self.link_id.clone(), lp);
            debug!("\t\t|  protocol-to-link");
            self.cc.start_timer(nw.clone()).await;
            let mut p2l_tx = self.p2l_tx.clone();
            match p2l_tx.send(ilp).await {
                Ok(_) => { },
                Err(e) => error!("protocol send error {:?}", e),
            }
            let ilp = self.cc.wait(nw.clone(), Arc::clone(&self.unreliable_unordered_response_rx)).await;
            match ilp {
                Some(ilp) => {
                    let nw = ilp.narrow_waist();
                    match nw.clone() {
                        NarrowWaistPacket::Request { .. } => { },
                        NarrowWaistPacket::Response { hbfi, .. } => {
                            let chunk = match hbfi.request_pid {
                                Some(_) => {
                                    nw.data(Some(self.protocol_sid.clone()))?
                                },
                                None => {
                                    nw.data(None)?
                                },
                            };
                            reconstruct.push(chunk);
                        },
                    }
                },
                None => {}
            }
            counter += 1;
        }
        Ok(reconstruct)
    }
    pub async fn unreliable_sequenced_request(&mut self, hbfi: HBFI, start: u64, end: u64) -> Result<Vec<Vec<u8>>> {
        let mut counter = start;
        let mut reconstruct: Vec<Vec<u8>> = vec![];
        while counter <= end {
            let hbfi_req = hbfi.clone().offset(counter);
            let nw = NarrowWaistPacket::request(hbfi_req)?;
            let lp = LinkPacket::new(self.link_id.reply_to()?, nw.clone());
            let ilp = InterLinkPacket::new(self.link_id.clone(), lp);
            debug!("\t\t|  protocol-to-link");
            self.cc.start_timer(nw.clone()).await;
            let mut p2l_tx = self.p2l_tx.clone();
            match p2l_tx.send(ilp).await {
                Ok(_) => { },
                Err(e) => error!("protocol send error {:?}", e),
            }
            let ilp = self.cc.wait(nw.clone(), Arc::clone(&self.unreliable_sequenced_response_rx)).await;
            match ilp {
                Some(ilp) => {
                    let nw = ilp.narrow_waist();
                    match nw.clone() {
                        NarrowWaistPacket::Request { .. } => { },
                        NarrowWaistPacket::Response { hbfi, .. } => {
                            if hbfi.ost < counter {
                                counter = hbfi.ost;
                                continue
                            }
                            let chunk = match hbfi.request_pid {
                                Some(_) => {
                                    nw.data(Some(self.protocol_sid.clone()))?
                                },
                                None => {
                                    nw.data(None)?
                                },
                            };
                            reconstruct.push(chunk);
                        },
                    }
                },
                None => {}
            }
            counter += 1;
        }
        Ok(reconstruct)
    }

    pub async fn reliable_unordered_request(&mut self, hbfi: HBFI, start: u64, end: u64) -> Result<Vec<Vec<u8>>> {
        let mut counter = start;
        let mut reconstruct: Vec<Vec<u8>> = vec![];
        while counter <= end {
            let hbfi_req = hbfi.clone().offset(counter);
            let nw = NarrowWaistPacket::request(hbfi_req)?;
            let lp = LinkPacket::new(self.link_id.reply_to()?, nw.clone());
            let ilp = InterLinkPacket::new(self.link_id.clone(), lp);
            debug!("\t\t|  protocol-to-link");
            self.cc.start_timer(nw.clone()).await;
            let mut p2l_tx = self.p2l_tx.clone();
            match p2l_tx.send(ilp).await {
                Ok(_) => { },
                Err(e) => error!("protocol send error {:?}", e),
            }
            let ilp = self.cc.wait(nw.clone(), Arc::clone(&self.reliable_unordered_response_rx)).await;
            match ilp {
                Some(ilp) => {
                    let nw = ilp.narrow_waist();
                    match nw.clone() {
                        NarrowWaistPacket::Request { .. } => { },
                        NarrowWaistPacket::Response { hbfi, .. } => {
                            let chunk = match hbfi.request_pid {
                                Some(_) => {
                                    nw.data(Some(self.protocol_sid.clone()))?
                                },
                                None => {
                                    nw.data(None)?
                                },
                            };
                            reconstruct.push(chunk);
                        },
                    }
                },
                None => {}
            }
            counter += 1;
        }
        Ok(reconstruct)
    }
    pub async fn reliable_ordered_request(&mut self, hbfi: HBFI, start: u64, end: u64) -> Result<Vec<Vec<u8>>> {
        let mut counter = start;
        let mut reconstruct: Vec<Vec<u8>> = vec![];
        while counter <= end {
            let hbfi_req = hbfi.clone().offset(counter);
            let nw = NarrowWaistPacket::request(hbfi_req)?;
            let lp = LinkPacket::new(self.link_id.reply_to()?, nw.clone());
            let ilp = InterLinkPacket::new(self.link_id.clone(), lp);
            debug!("\t\t|  protocol-to-link");
            self.cc.start_timer(nw.clone()).await;
            let mut p2l_tx = self.p2l_tx.clone();
            match p2l_tx.send(ilp).await {
                Ok(_) => { },
                Err(e) => error!("protocol send error {:?}", e),
            }
            let ilp = self.cc.wait(nw.clone(), Arc::clone(&self.reliable_ordered_response_rx)).await;
            match ilp {
                Some(ilp) => {
                    let nw = ilp.narrow_waist();
                    match nw.clone() {
                        NarrowWaistPacket::Request { .. } => { },
                        NarrowWaistPacket::Response { hbfi, .. } => {
                            let chunk = match hbfi.request_pid {
                                Some(_) => {
                                    nw.data(Some(self.protocol_sid.clone()))?
                                },
                                None => {
                                    nw.data(None)?
                                },
                            };
                            reconstruct.push(chunk);
                        },
                    }
                },
                None => {}
            }
            counter += 1;
        }
        Ok(reconstruct)
    }
    pub async fn reliable_sequenced_request(&mut self, hbfi: HBFI, start: u64, end: u64) -> Result<Vec<Vec<u8>>> {
        let mut counter = start;
        let mut reconstruct: Vec<Vec<u8>> = vec![];
        while counter <= end {
            let hbfi_req = hbfi.clone().offset(counter);
            let nw = NarrowWaistPacket::request(hbfi_req)?;
            let lp = LinkPacket::new(self.link_id.reply_to()?, nw.clone());
            let ilp = InterLinkPacket::new(self.link_id.clone(), lp);
            debug!("\t\t|  protocol-to-link");
            self.cc.start_timer(nw.clone()).await;
            let mut p2l_tx = self.p2l_tx.clone();
            match p2l_tx.send(ilp).await {
                Ok(_) => { },
                Err(e) => error!("protocol send error {:?}", e),
            }
            let ilp = self.cc.wait(nw.clone(), Arc::clone(&self.reliable_sequenced_response_rx)).await;
            match ilp {
                Some(ilp) => {
                    let nw = ilp.narrow_waist();
                    match nw.clone() {
                        NarrowWaistPacket::Request { .. } => { },
                        NarrowWaistPacket::Response { hbfi, .. } => {
                            if hbfi.ost < counter {
                                counter = hbfi.ost;
                                continue
                            }
                            let chunk = match hbfi.request_pid {
                                Some(_) => {
                                    nw.data(Some(self.protocol_sid.clone()))?
                                },
                                None => {
                                    nw.data(None)?
                                },
                            };
                            reconstruct.push(chunk);
                        },
                    }
                },
                None => {}
            }
            counter += 1;
        }
        Ok(reconstruct)
    }
    pub async fn respond(mut self,
        hbfi: HBFI,
        data: Vec<u8>,
    ) -> Result<()> {
        debug!("\t\t|  RESPONSE PACKET FOUND");
        let nw = NarrowWaistPacket::response(self.protocol_sid.clone(), hbfi.clone(), data, 0, 0)?;
        let lp = LinkPacket::new(self.link_id.reply_to()?, nw);
        let ilp = InterLinkPacket::new(self.link_id.clone(), lp);
        debug!("\t\t|  protocol-to-link");
        match self.p2l_tx.send(ilp).await {
            Ok(_) => {},
            Err(e) => error!("protocol send error {:?}", e),
        }
        Ok(())
    }
}