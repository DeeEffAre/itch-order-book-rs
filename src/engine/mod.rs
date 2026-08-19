use crate::{
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
    order_book::OrderBook,
};

#[derive(Default)]
struct Engine {
    books: Vec<Option<OrderBook>>,
}

impl Engine {
    #[inline]
    fn inline_get_book(&mut self, locate: usize) -> &mut OrderBook {
        if locate >= self.books.len() {
            self.books.resize_with(locate + 1, || None);
        }
        // if book does not exist then create one
        self.books[locate].get_or_insert_default()
    }
}

impl MessageHandler for Engine {
    fn on_add_order_no_mpid(&mut self, msg: &AddOrderNoMPID) {
        let locate = msg.stock_locate.get() as usize;
        self.inline_get_book(locate).on_add_order_no_mpid(msg);
    }

    fn on_add_order_with_mpid(&mut self, msg: &AddOrderWithMPID) {
        let locate = msg.stock_locate.get() as usize;
        self.inline_get_book(locate).on_add_order_with_mpid(msg);
    }

    fn on_system_event(&mut self, msg: &SystemEvent) {
        for book in self.books.iter_mut().flatten() {
            book.on_system_event(msg);
        }
    }

    fn on_order_cancel(&mut self, msg: &OrderCancel) {
        let locate = msg.stock_locate.get() as usize;
        self.inline_get_book(locate).on_order_cancel(msg);
    }

    fn on_order_delete(&mut self, msg: &OrderDelete) {
        let locate = msg.stock_locate.get() as usize;
        self.inline_get_book(locate).on_order_delete(msg);
    }

    fn on_order_replace(&mut self, msg: &OrderReplace) {
        let locate = msg.stock_locate.get() as usize;
        self.inline_get_book(locate).on_order_replace(msg);
    }

    fn on_order_execute(&mut self, msg: &OrderExecute) {
        let locate = msg.stock_locate.get() as usize;
        self.inline_get_book(locate).on_order_execute(msg);
    }

    fn on_order_execute_with_price(&mut self, msg: &OrderExecuteWithPrice) {
        let locate = msg.stock_locate.get() as usize;
        self.inline_get_book(locate)
            .on_order_execute_with_price(msg);
    }
}
