use std::ops::SubAssign;

use crate::itch_parser::{
    add_order::{AddOrderNoMPID, AddOrderWithMPID},
    modify_order::order_replace::OrderReplace,
};

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

impl SubAssign<u32> for Quantity {
    fn sub_assign(&mut self, rhs: u32) {
        self.0 = self.0.checked_sub(rhs).expect("Negative share amount");
    }
}

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

impl TryFrom<&OrderReplace> for Order {
    type Error = String;
    fn try_from(value: &OrderReplace) -> Result<Self, Self::Error> {
        Ok(Self {
            id: Id(value.new_order_ref_number.get()),
            price: Price::from(value.price.get()),
            shares: Quantity(value.shares.get()),
            side: Side::from_byte(b'S'), // dummy value
            timestamp: Timestamp(value.timestamp.get()),
        })
    }
}
