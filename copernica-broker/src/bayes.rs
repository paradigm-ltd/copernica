use {
    std::{
        collections::{
            HashMap,
            HashSet,
            hash_map::Keys
        },
        iter::FromIterator,
        vec::Vec
    },
    copernica_packets::{LinkId, HBFI, HBFIOnlyKeys},
};
struct BFIs {
    bfis: HashMap<HBFIOnlyKeys, HashMap<LinkId, i64>>,
}
impl BFIs {
    pub fn new() -> BFIs {
        BFIs {
            bfis: HashMap::new(),
        }
    }
    fn train(&mut self, bfis: &HBFI, link: &LinkId) {
        //debug!("train {:?}", bfis);
        let linkids = self.bfis
            .entry(HBFIOnlyKeys(bfis.clone()))
            .or_insert(HashMap::new());
        let value = linkids.entry(link.clone()).or_insert(0);
        *value += 1;
    }
    fn super_train(&mut self, bfis: &HBFI, link: &LinkId) {
        //debug!("supertrain {:?}", bfis);
        let linkids = self.bfis
            .entry(HBFIOnlyKeys(bfis.clone()))
            .or_insert(HashMap::new());
        let value = linkids.entry(link.clone()).or_insert(0);
        *value += 4;
    }
    fn get_frequency(&mut self, bfis: &HBFIOnlyKeys, linkid: &LinkId) -> (Option<&i64>, bool) {
        match self.bfis.get(bfis) {
            Some(linkids) => match linkids.get(linkid) {
                Some(value) => return (Some(value), true),
                None => return (None, true),
            },
            None => return (None, false),
        }
    }
}
struct Links {
    count: HashMap<LinkId, i64>,
}
impl Links {
    pub fn new() -> Links {
        Links {
            count: HashMap::new(),
        }
    }
    fn train(&mut self, link: &LinkId) {
        let value = self.count.entry(link.clone()).or_insert(0);
        *value += 1;
    }
    fn super_train(&mut self, link: &LinkId) {
        let value = self.count.entry(link.clone()).or_insert(0);
        *value += 4;
    }
    fn get_count(&mut self, link: &LinkId) -> Option<&i64> {
        return self.count.get(link);
    }
    fn get_linkids(&mut self) -> Keys<LinkId, i64> {
        return self.count.keys();
    }
    fn get_total(&mut self) -> i64 {
        return self.count.values().fold(0, |acc, x| acc + x);
    }
}
struct Model {
    links: Links,
    bfis: BFIs,
}
impl Model {
    pub fn new() -> Model {
        Model {
            links: Links::new(),
            bfis: BFIs::new(),
        }
    }
    fn add_link(&mut self, linkid: &LinkId) {
        self.links.train(linkid);
    }
    fn train(&mut self, data: &HBFI, linkid: &LinkId) {
        self.links.train(linkid);
        self.bfis.train(data, linkid);
    }
    fn super_train(&mut self, data: &HBFI, linkid: &LinkId) {
        self.links.super_train(linkid);
        self.bfis.super_train(data, linkid);
    }
}
#[derive(Debug)]
pub struct LinkWeight{
    pub linkid: LinkId,
    pub weight: f64,
}
pub struct Bayes {
    model: Model,
    min_prob: f64,
    min_log_prob: f64
}
impl Bayes {
    pub fn new() -> Bayes {
        Bayes {
            model: Model::new(),
            min_prob: 1e-9,
            min_log_prob: -100.0,
        }
    }
    pub fn add_link(&mut self, linkid: &LinkId) {
        self.model.add_link(&linkid);
    }
    fn prior(&mut self, linkid: &LinkId) -> Option<f64> {
        let total = *(&self.model.links.get_total()) as f64;
        let linkid = &self.model.links.get_count(linkid);
        if linkid.is_some() && total > 0.0 {
            return Some(*linkid.unwrap() as f64 / total);
        } else {
            return None;
        }
    }
    fn log_prior(&mut self, linkid: &LinkId) -> Option<f64> {
        let total = *(&self.model.links.get_total()) as f64;
        let linkid = &self.model.links.get_count(linkid);
        if linkid.is_some() && total > 0.0 {
            return Some((*linkid.unwrap() as f64).ln() - total.ln());
        } else {
            return None;
        }
    }
    fn calculate_attr_prob(&mut self, bfis: &HBFIOnlyKeys, linkid: &LinkId) -> Option<f64> {
        match self.model.bfis.get_frequency(bfis, linkid) {
            (Some(frequency), true) => match self.model.links.get_count(linkid) {
                Some(count) => return Some((*frequency as f64) / (*count as f64)),
                None => return None,
            },
            (None, true) => return Some(self.min_prob),
            (None, false) => return None,
            (Some(_), false) => None,
        }
    }
    fn calculate_attr_log_prob(&mut self, bfis: &HBFIOnlyKeys, linkid: &LinkId) -> Option<f64> {
        match self.model.bfis.get_frequency(bfis, linkid) {
            (Some(frequency), true) => match self.model.links.get_count(linkid) {
                Some(count) => return Some((*frequency as f64).ln() - (*count as f64).ln()),
                None => return None,
            },
            (None, true) => return Some(self.min_log_prob),
            (None, false) => return None,
            (Some(_), false) => None,
        }
    }
    fn link_prob(&mut self, linkid: &LinkId, bfismap: &HashSet<HBFIOnlyKeys>) -> Vec<f64> {
        let mut probs: Vec<f64> = Vec::new();
        for bfis in bfismap {
            match self.calculate_attr_prob(bfis, linkid) {
                Some(p) => {
                    probs.push(p);
                }
                None => {}
            }
        }
        return probs;
    }
    fn link_log_prob(&mut self, linkid: &LinkId, bfismap: &HashSet<HBFIOnlyKeys>) -> Vec<f64> {
        let mut probs: Vec<f64> = Vec::new();
        for bfis in bfismap {
            match self.calculate_attr_log_prob(bfis, linkid) {
                Some(p) => {
                    probs.push(p);
                }
                None => {}
            }
        }
        return probs;
    }
    pub fn train(&mut self, data: &HBFI, linkid: &LinkId) {
        self.model.train(data, linkid);
    }
    pub fn super_train(&mut self, data: &HBFI, linkid: &LinkId) {
        self.model.super_train(data, linkid);
    }
    pub fn classify(&mut self, data: &HBFI) -> Vec<LinkWeight> {
        let mut bfis_set: HashSet<HBFIOnlyKeys> = HashSet::new();
        bfis_set.insert(HBFIOnlyKeys(data.clone()));
        let mut result: Vec<LinkWeight> = vec![];
        let linkids: HashSet<LinkId> =
            HashSet::from_iter(self.model.links.get_linkids().into_iter().cloned());
        for linkid in linkids {
            let p = self.link_prob(&linkid, &bfis_set);
            let p_iter = p.into_iter().fold(1.0, |acc, x| acc * x);
            let weight = p_iter * self.prior(&linkid).unwrap();
            let linkid = linkid.clone();
            let lw = LinkWeight { linkid, weight };
            result.push(lw);
        }
        result.sort_by(|a, b| a.weight.total_cmp(&b.weight));
        result.reverse();
        result
    }
    /// classify a `BFIS` returning a map of links and log-probabilities
    /// as keys and values, respectively. Using `log_classify` may prevent underflows.
    pub fn log_classify(&mut self, data: &HBFIOnlyKeys) -> Vec<LinkWeight> {
        let mut bfis_set: HashSet<HBFIOnlyKeys> = HashSet::new();
        bfis_set.insert(data.clone());
        let mut result: Vec<LinkWeight> = vec![];
        let linkids: HashSet<LinkId> =
            HashSet::from_iter(self.model.links.get_linkids().into_iter().cloned());
        for linkid in linkids {
            let p = self.link_log_prob(&linkid, &bfis_set);
            let max = p.iter().cloned().fold(-1./0. /* inf */, f64::max);
            let p_iter = p.into_iter().fold(0.0, |acc, x| acc + (x - max).exp());
            let weight = max + p_iter.ln() + self.log_prior(&linkid).unwrap();
            let linkid = linkid.clone();
            result.push(LinkWeight { linkid, weight });
        }
        result.sort_by(|a, b| a.weight.total_cmp(&b.weight));
        result.reverse();
        result
    }
}
/*
#[cfg(test)]
mod test_bfis {
    use super::*;
    use copernica_packets::{BFIS, LinkId, ReplyTo, constants, PrivateIdentityInterface};
    fn generate_bfis() -> BFIS {
        let h1: BFIS = [
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
        ];
        h1
    }
    #[test]
    fn bfi_add() {
        let mut model = BFIs::new();
        let h1 = generate_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let li = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        model.train(&h1, &li);
        assert_eq!(
            *model
                .get_frequency(&h1, &li)
                .0
                .unwrap(),
            1
        );
    }
    #[test]
    fn get_non_existing() {
        let mut model = BFIs::new();
        let h1 = generate_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let li = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        assert_eq!(
            model
                .get_frequency(&h1, &li)
                .0,
            None
        );
    }
}
#[cfg(test)]
mod test_linkids {
    use super::*;
    use copernica_packets::{LinkId, ReplyTo, PrivateIdentityInterface};
    #[test]
    fn linkid_add() {
        let mut linkids = Links::new();
        let private_identity = PrivateIdentityInterface::new_key();
        let h1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        linkids.train(&h1);
        assert_eq!(*linkids.get_count(&h1).unwrap(), 1);
    }
    #[test]
    fn linkid_get_nonexistent() {
        let mut linkids = Links::new();
        let private_identity = PrivateIdentityInterface::new_key();
        let h1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        assert_eq!(linkids.get_count(&h1), None);
    }
    #[test]
    fn get_linkids() {
        let mut linkids = Links::new();
        let private_identity = PrivateIdentityInterface::new_key();
        let h1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        linkids.train(&h1);
        assert_eq!(linkids.get_linkids().len(), 1);
        assert_eq!(linkids.get_linkids().last().unwrap(), &h1);
    }
    #[test]
    fn get_counts() {
        let mut linkids = Links::new();
        let private_identity = PrivateIdentityInterface::new_key();
        let h1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        linkids.train(&h1);
        linkids.train(&h1);
        assert_eq!(linkids.get_linkids().len(), 1);
        assert_eq!(*linkids.get_count(&h1).unwrap(), 2);
    }
    #[test]
    fn get_nonexistent_counts() {
        let mut linkids = Links::new();
        let private_identity = PrivateIdentityInterface::new_key();
        let h1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        assert_eq!(linkids.get_linkids().len(), 0);
        assert_eq!(linkids.get_count(&h1), None);
    }
    #[test]
    fn get_nonexistent_total() {
        let mut linkids = Links::new();
        assert_eq!(linkids.get_total(), 0);
    }
    #[test]
    fn get_total() {
        let mut linkids = Links::new();
        let private_identity = PrivateIdentityInterface::new_key();
        let h1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        let private_identity = PrivateIdentityInterface::new_key();
        let h2 = LinkId::listen(private_identity, None, ReplyTo::Rf(1));
        let private_identity = PrivateIdentityInterface::new_key();
        let h3 = LinkId::listen(private_identity, None, ReplyTo::Rf(2));
        linkids.train(&h1);
        linkids.train(&h1);
        linkids.train(&h2);
        linkids.train(&h3);
        assert_eq!(linkids.get_total(), 4);
    }
}
#[cfg(test)]
mod test_bayes {
    use super::*;
    use std::f64::consts::LN_2;
    use copernica_packets::{LinkId, ReplyTo, constants, PrivateIdentityInterface};
    fn generate_max_bfis() -> BFIS {
        let h1: BFIS = [
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
        ];
        h1
    }
    fn generate_mid_bfis() -> BFIS {
        let h1: BFIS = [
            [u16::MAX/2; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX/2; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX/2; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX/2; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX/2; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MAX/2; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
        ];
        h1
    }
    fn generate_min_bfis() -> BFIS {
        let h1: BFIS = [
            [u16::MIN; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MIN; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MIN; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MIN; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MIN; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
            [u16::MIN; constants::BLOOM_FILTER_INDEX_ELEMENT_LENGTH as usize],
        ];
        h1
    }
    #[test]
    fn test_prior() {
        let mut nb = Bayes::new();
        let h1: BFIS = generate_min_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let l1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        nb.model.train(&h1, &l1);
        let prior = nb.prior(&l1);
        assert_eq!(prior, Some(1.0));
    }
    #[test]
    fn test_log_prior() {
        let mut nb = Bayes::new();
        let h1: BFIS = generate_max_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let l1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        nb.model.train(&h1, &l1);
        let prior = nb.log_prior(&l1);
        assert_eq!(prior, Some(0.0));
    }
    #[test]
    fn test_prior_nonexistent() {
        let mut nb = Bayes::new();
        let h1: BFIS = generate_min_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let l1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        let private_identity = PrivateIdentityInterface::new_key();
        let l2 = LinkId::listen(private_identity, None, ReplyTo::Rf(1));
        nb.model.train(&h1, &l1);
        let prior = nb.prior(&l2);
        assert_eq!(prior, None);
    }
    #[test]
    fn test_classification() {
        let mut nb = Bayes::new();
        let h1: BFIS = generate_min_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let l1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        nb.model.train(&h1, &l1);
        let h2: BFIS = generate_mid_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let l2 = LinkId::listen(private_identity, None, ReplyTo::Rf(1));
        nb.model.train(&h2, &l2);
        /*let h3: BFIS = generate_max_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let l3 = LinkId::listen(private_identity, None, ReplyTo::Rf(2));
        nb.model.train(&h3, &l3);*/
        let classes = nb.classify(&h1);
        assert_eq!(classes[0].weight, 0.5f64);
        assert_eq!(classes[1].weight, 0.0000000005f64);
    }
    #[test]
    fn test_log_classification() {
        let mut nb = Bayes::new();
        let h1: BFIS = generate_min_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let l1 = LinkId::listen(private_identity, None, ReplyTo::Rf(0));
        nb.model.train(&h1, &l1);
        let h2: BFIS = generate_mid_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let l2 = LinkId::listen(private_identity, None, ReplyTo::Rf(1));
        nb.model.train(&h2, &l2);
        /*let h3: BFIS = generate_max_bfis();
        let private_identity = PrivateIdentityInterface::new_key();
        let l3 = LinkId::listen(private_identity, None, ReplyTo::Rf(2));
        nb.model.train(&h3, &l3);*/
        let classes = nb.log_classify(&h1);
        assert_eq!(classes[0].weight, -LN_2);
        assert_eq!(classes[1].weight, -100.69314718055995);
    }
}
*/
