use crate::{
    GlobalMap,
    itch_parser::{
        add_order::{AddOrderNoMPID, AddOrderWithMPID},
        handler::MessageHandler,
        modify_order::{
            order_cancel::OrderCancel,
            order_delete::OrderDelete,
            order_execute::{OrderExecute, OrderExecuteWithPrice},
            order_replace::OrderReplace,
        },
        system_event_message::SystemEvent,
    },
    order_book::{OrderBook, SlabIndex},
};

#[derive(Default)]
pub struct Engine {
    pub books: Vec<Option<OrderBook>>,
    /// global order_ref -> (locate, slab index) lookup; one array for all books
    /// because ITCH order reference numbers are a dense global counter
    pub global_map: GlobalMap,
}

impl Engine {
    #[inline]
    pub fn inline_get_book(&mut self, locate: usize) -> &mut OrderBook {
        if locate >= self.books.len() {
            self.books.resize_with(locate + 1, || None);
        }
        // if book does not exist then create one
        self.books[locate].get_or_insert_default()
    }
}

impl MessageHandler for Engine {
    fn on_add_order_no_mpid(&mut self, msg: &AddOrderNoMPID) -> Option<SlabIndex> {
        let locate = msg.stock_locate.get();
        let slab = self.inline_get_book(locate as usize).add_order_no_mpid(msg);
        if let Some(slab) = slab {
            self.global_map.set(msg.order_ref_number.get(), locate, slab);
        }
        slab
    }

    fn on_add_order_with_mpid(&mut self, msg: &AddOrderWithMPID) -> Option<SlabIndex> {
        let locate = msg.stock_locate.get();
        let slab = self.inline_get_book(locate as usize).add_order_with_mpid(msg);
        if let Some(slab) = slab {
            self.global_map.set(msg.order_ref_number.get(), locate, slab);
        }
        slab
    }

    fn on_order_cancel(&mut self, msg: &OrderCancel) {
        let Some((locate, slab)) = self.global_map.get(msg.order_ref_number.get()) else {
            return;
        };
        let fully_done = self
            .inline_get_book(locate as usize)
            .handle_execution_slab(slab, msg.cancelled_shares.get());
        if fully_done {
            self.global_map.remove(msg.order_ref_number.get());
        }
    }

    fn on_order_delete(&mut self, msg: &OrderDelete) {
        let Some((locate, slab)) = self.global_map.get(msg.order_ref_number.get()) else {
            return;
        };
        self.inline_get_book(locate as usize).remove_slab(slab);
        self.global_map.remove(msg.order_ref_number.get());
    }

    fn on_order_replace(&mut self, msg: &OrderReplace) {
        let Some((locate, old_slab)) = self.global_map.get(msg.org_order_ref_number.get()) else {
            return;
        };
        let new_slab = self
            .inline_get_book(locate as usize)
            .replace_slab(old_slab, msg);
        if let Some(new_slab) = new_slab {
            self.global_map.set(msg.new_order_ref_number.get(), locate, new_slab);
        }
        self.global_map.remove(msg.org_order_ref_number.get());
    }

    fn on_order_execute(&mut self, msg: &OrderExecute) {
        let Some((locate, slab)) = self.global_map.get(msg.order_ref_number.get()) else {
            return;
        };
        let fully_done = self
            .inline_get_book(locate as usize)
            .handle_execution_slab(slab, msg.executed_shares.get());
        if fully_done {
            self.global_map.remove(msg.order_ref_number.get());
        }
    }

    fn on_order_execute_with_price(&mut self, msg: &OrderExecuteWithPrice) {
        let Some((locate, slab)) = self.global_map.get(msg.order_ref_number.get()) else {
            return;
        };
        let fully_done = self
            .inline_get_book(locate as usize)
            .handle_execution_slab(slab, msg.executed_shares.get());
        if fully_done {
            self.global_map.remove(msg.order_ref_number.get());
        }
    }

    fn on_system_event(&mut self, msg: &SystemEvent) {
        match msg.event_code {
            b'O' | b'S' | b'E' | b'C' => self.global_map.clear(),
            _ => {}
        }
        for book in self.books.iter_mut().flatten() {
            book.on_system_event(msg);
        }
    }
}
