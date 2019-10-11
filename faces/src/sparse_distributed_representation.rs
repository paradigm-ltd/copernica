use packets::{Packet};

use bitvec::prelude::*;
use rand::Rng;

#[derive(Debug, Clone)]
pub struct SparseDistributedRepresentation {
    sdr: BitVec,
}

impl SparseDistributedRepresentation {
    pub fn new() -> Self {
        SparseDistributedRepresentation {
            sdr: bitvec![0; 2048],
        }
    }

    pub fn insert(&mut self, packet: Packet) {
        let mut i: Vec<Vec<u16>> = Vec::new();
        match packet {
            Packet::Interest { name: _, sdri } => {
                i = sdri;
            },
            Packet::Data { name: _, sdri } => {
                i = sdri;
            },
        }
        for row in i {
            for elem in row {
                self.sdr.set(elem as usize, true);
            }
        }
    }

    pub fn contains(&mut self, packet: Packet) -> u8 {
        let mut i: Vec<Vec<u16>> = Vec::new();
        let mut sdr_vals: Vec<u32> = Vec::new();
        match packet {
            Packet::Interest { name: _, sdri } => {
                i = sdri;
            },
            Packet::Data     { name: _, sdri } => {
                i = sdri;
            },
        }
        for row in i {
            for elem in row {
                sdr_vals.push(self.sdr.get(elem as usize).unwrap() as u32);
            }
        }
        let hits = sdr_vals.iter().try_fold(0u32, |acc, &elem| acc.checked_add(elem));
        let percentage = (hits.unwrap() as f32 / sdr_vals.len() as f32) * 100f32;
        //println!("hits: {:?}, length: {:?}, percentage: {}", hits.unwrap(), vals.len(), percentage);
        percentage as u8
    }

    pub fn delete(&mut self, packet: Packet) {
        let mut i: Vec<Vec<u16>> = Vec::new();
        match packet {
            Packet::Interest { name: _, sdri } => {
                i = sdri;
            },
            Packet::Data     { name: _, sdri } => {
                i = sdri;
            },
        }
        for row in i {
            for elem in row {
                self.sdr.set(elem as usize, false);
            }
        }
    }

    pub fn restore(&mut self) {
        let mut rng = rand::thread_rng();
        for _ in 0 .. 2048 {
            self.sdr.set(rng.gen_range(0, 2048), false);
        }
    }

    pub fn decoherence(&mut self) -> u8 {
        let hits = self.sdr.iter().try_fold(0u32, |acc, elem| acc.checked_add(elem as u32));
        let percentage = (hits.unwrap() as f32 / self.sdr.len() as f32) * 100f32;
        //println!("hits: {:?}, length: {:?}, percentage: {}", hits.unwrap(), vals.len(), percentage);
        percentage as u8
    }

    #[cfg(test)]
    pub fn print(&self) {
        println!("{:?}", self.sdr);
    }

    #[cfg(test)]
    pub fn fill_1s(&mut self) {
        for i in 0 .. 2048 {
            self.sdr.set(i, true);
        }
    }
}

impl PartialEq for SparseDistributedRepresentation {
    fn eq(&self, other: &SparseDistributedRepresentation) -> bool {
        self.sdr == other.sdr
    }
}


#[cfg(test)]
mod sdr {
    use super::*;
    use packets::{ mk_interest};

    #[test]
    fn interest_100_percent_present() {
        let interest = mk_interest("interested/in/world/affairs".to_string());
        let mut sdr = SparseDistributedRepresentation::new();
        sdr.insert(interest.clone());
        assert_eq!(sdr.contains(interest), 100);
    }

    #[test]
    fn decoherence_restoration() {
        let mut sdr = SparseDistributedRepresentation::new();
        sdr.fill_1s();
        sdr.restore();
        assert!(sdr.decoherence() < 40);
    }
}
