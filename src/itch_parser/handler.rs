use crate::order_book::SlabIndex;

use super::{
    add_order::{AddOrderNoMPID, AddOrderWithMPID},
    modify_order::{
        order_cancel::OrderCancel,
        order_delete::OrderDelete,
        order_execute::{OrderExecute, OrderExecuteWithPrice},
        order_replace::OrderReplace,
    },
    system_event_message::SystemEvent,
};

#[allow(unused_variables)]
pub trait MessageHandler {
    fn on_system_event(&mut self, msg: &SystemEvent) {}
    fn on_add_order_no_mpid(&mut self, msg: &AddOrderNoMPID) -> Option<SlabIndex> {
        return None;
    }
    fn on_add_order_with_mpid(&mut self, msg: &AddOrderWithMPID) -> Option<SlabIndex> {
        return None;
    }
    fn on_order_cancel(&mut self, msg: &OrderCancel) {}
    fn on_order_delete(&mut self, msg: &OrderDelete) {}
    fn on_order_replace(&mut self, msg: &OrderReplace) {}
    fn on_order_execute(&mut self, msg: &OrderExecute) {}
    fn on_order_execute_with_price(&mut self, msg: &OrderExecuteWithPrice) {}
}
