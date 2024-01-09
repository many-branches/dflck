use serde::{Serialize, Deserialize};
use std::{time::{SystemTime, UNIX_EPOCH}, collections::BTreeSet};
use sha2::{Sha256, Digest};
use rand::{Rng, distributions::Standard};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Address(Vec<u8>);

impl Address {
    pub fn random<R: Rng>(rng: &mut R) -> Self {
        Self(rng.sample_iter(Standard).take(32).collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Signer(Address);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Signature(Vec<u8>);

impl Signature {
    pub fn random<R: Rng>(rng: &mut R) -> Self {
        Self(rng.sample_iter(Standard).take(32).collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Label(Vec<u8>);

impl Label {
    fn random<R: Rng>(rng: &mut R) -> Self {
        let len = rng.gen_range(1..32);
        let bytes: Vec<u8> = rng.sample_iter(Standard).take(len).collect();
        Label(bytes)
    }
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum Argument {
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Address(Address),
    Signer(Signer),
    Vector(Vec<Argument>),
    Struct(Vec<Argument>),
}

impl Argument {
    pub fn random<R: Rng>(rng: &mut R, depth: u64) -> Self {

        let partition = if depth > 0 { 9 } else { 7 };  // Adjust based on how many options you have

        match rng.gen_range(0..partition) {
            0 => Argument::U8(rng.gen()),
            1 => Argument::U16(rng.gen()),
            2 => Argument::U32(rng.gen()),
            3 => Argument::U64(rng.gen()),
            4 => Argument::U128(rng.gen()),
            5 => Argument::Address(Address::random(rng)),
            6 => Argument::Signer(Signer(Address::random(rng))),
            7 => {
                let len = rng.gen_range(1..8);
                let args: Vec<Argument> = (0..len).map(|_| Argument::random(rng, depth - 1)).collect();
                Argument::Vector(args)
            },
            8 => {
                let len = rng.gen_range(1..8);
                let args: Vec<Argument> = (0..len).map(|_| Argument::random(rng, depth - 1)).collect();
                Argument::Struct(args)
            },
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum TypeArgument {
    U8,
    U16,
    U32,
    U64,
    U128,
    Address,
    Signer,
    Vector(Box<TypeArgument>),
    Struct(Vec<TypeArgument>),
}

impl TypeArgument {
    pub fn random<R: Rng>(rng: &mut R, depth : u64) -> Self {

        let partition = if depth > 0 { 9 } else { 7 };

        match rng.gen_range(0..partition) { // Adjusted the range to 0..9 to include the 8th option
            0 => TypeArgument::U8,
            1 => TypeArgument::U16,
            2 => TypeArgument::U32,
            3 => TypeArgument::U64,
            4 => TypeArgument::U128,
            5 => TypeArgument::Address,
            6 => TypeArgument::Signer,
            7 => TypeArgument::Vector(Box::new(TypeArgument::random(rng, depth - 1))),
            8 => {
                let len = rng.gen_range(1..8);
                let args: Vec<TypeArgument> = (0..len).map(|_| TypeArgument::random(rng, depth - 1)).collect();
                TypeArgument::Struct(args)
            },
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize,  PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct ExecutionPayload {
    pub address : Address,
    pub label : Label,
    pub arguments : Vec<Argument>,
    pub type_arguments : Vec<TypeArgument>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum TransactionPayload {
    ExecutionPayload(ExecutionPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Transaction {
    pub timestamp: u64,
    pub nonce : u64,
    pub payload: TransactionPayload,
    pub signers : Vec<Vec<Address>>,
    pub signatures : Vec<Vec<Signature>>,
}

impl Transaction {

    pub fn new(
        timestamp: u64,
        nonce : u64,
        payload: TransactionPayload,
        signers : Vec<Vec<Address>>,
        signatures : Vec<Vec<Signature>>,
    ) -> Self {
        Self {
            timestamp,
            nonce,
            payload,
            signers,
            signatures,
        }
    }

    pub fn random<R: Rng>(rng: &mut R) -> Self {
        let timestamp = rng.gen();
        let nonce = rng.gen();
        let payload = TransactionPayload::ExecutionPayload(ExecutionPayload {
            address : Address::random(rng),
            label : Label::random(rng),
            arguments : vec![Argument::random(rng, 8)],
            type_arguments : vec![TypeArgument::random(rng, 8)],
        });
        let signers = vec![vec![Address::random(rng)]];
        let signatures = vec![vec![Signature::random(rng)]];
        Self {
            timestamp,
            nonce,
            payload,
            signers,
            signatures,
        }
    }

    pub fn random_now<R: Rng>(rng: &mut R) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let nonce = rng.gen();
        let payload = TransactionPayload::ExecutionPayload(ExecutionPayload {
            address : Address::random(rng),
            label : Label::random(rng),
            arguments : vec![Argument::random(rng, 8)],
            type_arguments : vec![TypeArgument::random(rng, 8)],
        });
        let signers = vec![vec![Address::random(rng)]];
        let signatures = vec![vec![Signature::random(rng)]];
        Self {
            timestamp,
            nonce,
            payload,
            signers,
            signatures,
        }
    }

    pub fn hash(&self) -> String {
        let input_str = format!(
            "{}{}{:?}",
            &self.timestamp, 
            &self.nonce, 
            &self.payload
        );
        let mut hasher = Sha256::new();
        hasher.update(input_str);
        format!("{:x}", hasher.finalize())
    }

    pub fn display_hash(&self) -> String {
        // shorten the hash for display
        let hash = self.hash();
        let mut display_hash = String::new();
        for (i, c) in hash.chars().enumerate() {
            if i % 8 == 0 {
                display_hash.push_str(" ");
            }
            display_hash.push(c);
        }
        display_hash
    }

}


#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct Block {
    pub timestamp: u64,
    pub slot_number: u64, // this is derived from timestamp, but should be 
    pub transactions: BTreeSet<Transaction>,
    pub nonce: u64,
    pub previous_hash: String,
    pub hash: String,
}

impl Block {
    pub fn new(transactions : BTreeSet<Transaction>, previous_hash: &str, slot_secs : u64) -> Block {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        
        let mut block = Block {
            timestamp,
            slot_number : timestamp / slot_secs,
            transactions,
            previous_hash: previous_hash.to_string(),
            hash: String::new(),
            nonce: 0,
        };

        block.hash = block.calculate_hash();
        block
    }

    pub fn genesis(slot_secs : u64) -> Block {
        let mut transactions = BTreeSet::new();
        let timestamp = 0;
        let slot_number = timestamp / slot_secs;
        let nonce = 0;
        let previous_hash = String::new();
        let hash = String::new();
        let mut block = Block {
            timestamp,
            slot_number,
            transactions,
            nonce,
            previous_hash,
            hash,
        };
        block.hash = block.calculate_hash();
        block
    }

    pub fn calculate_hash(&self) -> String {
        let input_str = format!(
            "{}{}{}",
            &self.timestamp, 
            &self.previous_hash, 
            &self.nonce
        );
        let mut hasher = Sha256::new();
        hasher.update(input_str);
        for txn in &self.transactions {
            hasher.update(format!("{:?}", txn));
        }
        format!("{:x}", hasher.finalize())
    }

    pub fn check_hash(&self) -> bool {
        self.hash == self.calculate_hash()
    }

    pub fn add_transaction(&mut self, transaction: Transaction) -> bool {
        self.transactions.insert(transaction)
    }

    pub fn drop_transaction(&mut self, transaction: &Transaction) -> bool {
        self.transactions.remove(transaction)
    }

    pub fn mine_block(&mut self, difficulty: usize) {
        self.hash = self.calculate_hash(); // make sure the hash is set
        let target = String::from_utf8(vec![b'0'; difficulty]).unwrap();
        while &self.hash[..difficulty] != target {
            self.nonce += 1;
            self.hash = self.calculate_hash();
        }
    }

    pub fn verify_proof_of_work(&self, difficulty: usize) -> bool {
        let target = String::from_utf8(vec![b'0'; difficulty]).unwrap();
        &self.hash[..difficulty] == target
    }

    pub fn compute_slot_number(&self, slot_secs : u64) -> u64 {
        self.timestamp / slot_secs
    }

    pub fn pre_verify_self(&self, slot_secs : u64) -> bool {
        self.check_hash() && 
        (self.compute_slot_number(slot_secs) == self.slot_number)
    }

    pub fn verify_self(&self, slot_secs : u64, difficulty: usize) -> bool {
        self.check_hash() && 
        (self.compute_slot_number(slot_secs) == self.slot_number) &&
        self.verify_proof_of_work(difficulty)
    }

    pub fn random<R: Rng>(rng: &mut R, slot_secs : u64) -> Self {
        let timestamp = rng.gen();
        let mut transactions = BTreeSet::new();
        for _ in 0..rng.gen_range(0..32) {
            transactions.insert(Transaction::random(rng));
        }
        let nonce = rng.gen();
        let previous_hash = String::new();
        let hash = String::new();
        let mut block = Self {
            timestamp,
            slot_number : timestamp / slot_secs,
            transactions,
            nonce,
            previous_hash,
            hash,
        };
        block.hash = block.calculate_hash();
        block
    }

}

#[cfg(test)]
mod test {

    use super::*;

    #[tokio::test]
    async fn test_mine_block() {
        let slot_secs = 12;
        let difficulty = 2;
        let mut block = Block::new(BTreeSet::new(), "0", slot_secs);
        block.mine_block(difficulty);
        assert!(block.pre_verify_self(slot_secs));
        assert!(block.verify_proof_of_work(2));
        assert!(block.verify_self(slot_secs, difficulty));
    }

    #[tokio::test]
    async fn test_randomized_mine_block() {
        let slot_secs = 12;
        let difficulty = 2;
        let rng = &mut rand::thread_rng();
        for _ in 0..rng.gen_range(8..32) {
            let mut block = Block::random(rng, slot_secs);
            block.mine_block(difficulty);
            assert!(block.pre_verify_self(slot_secs));
            assert!(block.verify_proof_of_work(2));
            assert!(block.verify_self(slot_secs, difficulty));
        }
    }

    #[tokio::test]
    async fn test_different_transactions_are_different() {
        let mut rng = rand::thread_rng();
        let txn1 = Transaction::random_now(&mut rng);
        let txn2 = Transaction::random_now(&mut rng);
        assert_ne!(txn1, txn2);
    }

    #[tokio::test]
    async fn test_btree() {
        let mut set = BTreeSet::new();
        let mut rng = rand::thread_rng();
        for _ in 0..32 {
            let txn = Transaction::random_now(&mut rng);
            assert!(!set.contains(&txn));
            set.insert(txn.clone());
        }
    }


}
