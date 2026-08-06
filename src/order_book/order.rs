use crate::itch_parser::add_order::{AddOrderNoMPID, AddOrderWithMPID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Id(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Price(pub u64);

impl From<u32> for Price {
    fn from(value: u32) -> Self {
        Self((value * 10000).into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub u64);

impl Side {
    fn from_byte(raw_byte: u8) -> Self {
        match raw_byte {
            b'B' => Side::Buy,
            b'S' => Side::Sell,
            _ => panic!("Invalid add order side"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: Id,
    /// scaled integer price
    pub price: Price,
    pub shares: Quantity,
    pub side: Side,
    pub timestamp: Timestamp,
}

impl TryFrom<&AddOrderNoMPID> for Order {
    type Error = String;
    fn try_from(value: &AddOrderNoMPID) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Id(value.order_ref_number.get()),
            price: Price::from(value.price.get()),
            shares: Quantity(value.shares.get()),
            side: Side::from_byte(value.indicator),
            timestamp: Timestamp(value.timestamp.get()),
        })
    }
}

impl TryFrom<&AddOrderWithMPID> for Order {
    type Error = String;
    fn try_from(value: &AddOrderWithMPID) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Id(value.order_ref_number.get()),
            price: Price::from(value.price.get()),
            shares: Quantity(value.shares.get()),
            side: Side::from_byte(value.indicator),
            timestamp: Timestamp(value.timestamp.get()),
        })
    }
}
