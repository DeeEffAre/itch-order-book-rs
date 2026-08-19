#![allow(clippy::inline_always)]
use crate::itch_parser::{
    add_order::{AddOrderNoMPID, AddOrderWithMPID},
    handler::MessageHandler,
    modify_order::{
        order_cancel::OrderCancel,
        order_delete::OrderDelete,
        order_execute::{OrderExecute, OrderExecuteWithPrice},
        order_replace::OrderReplace,
    },
    system_event_message::SystemEvent,
};

pub mod add_order;
pub mod handler;
pub mod modify_order;
pub mod system_event_message;
pub mod types;

pub trait Message {
    fn new(raw_message: &[u8]) -> Option<&Self>;
}

macro_rules! impl_dispatch {
    ($handler:expr, $raw:expr, [
        $( $byte:literal => $ty:ty => $method:ident ),+ $(,)?
    ]) => {
        match $raw[0] {
            $( $byte => {
                if let Some(msg) = <$ty>::new($raw) {
                    $handler.$method(msg);
                }
            } )+
            _ => {}
        }
    };
}

#[inline(always)]
pub fn parse_and_dispatch<H: MessageHandler>(raw: &[u8], handler: &mut H) {
    impl_dispatch!(handler, raw, [
        b'S' => SystemEvent            => on_system_event,
        b'A' => AddOrderNoMPID         => on_add_order_no_mpid,
        b'F' => AddOrderWithMPID       => on_add_order_with_mpid,
        b'X' => OrderCancel            => on_order_cancel,
        b'D' => OrderDelete            => on_order_delete,
        b'U' => OrderReplace           => on_order_replace,
        b'E' => OrderExecute           => on_order_execute,
        b'C' => OrderExecuteWithPrice  => on_order_execute_with_price,
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::order_book::SlabIndex;

    fn write_u16_be(buf: &mut [u8], offset: usize, val: u16) {
        buf[offset..offset + 2].copy_from_slice(&val.to_be_bytes());
    }

    fn write_u32_be(buf: &mut [u8], offset: usize, val: u32) {
        buf[offset..offset + 4].copy_from_slice(&val.to_be_bytes());
    }

    fn write_u48_be(buf: &mut [u8], offset: usize, val: u64) {
        let bytes = val.to_be_bytes();
        buf[offset..offset + 6].copy_from_slice(&bytes[2..8]);
    }

    fn write_u64_be(buf: &mut [u8], offset: usize, val: u64) {
        buf[offset..offset + 8].copy_from_slice(&val.to_be_bytes());
    }

    fn is_zero_copy<T>(original_buf: &[u8], reference: &T) -> bool {
        let buf_start = original_buf.as_ptr() as usize;
        let buf_end = buf_start + original_buf.len();
        let ref_addr = std::ptr::from_ref::<T>(reference) as usize;
        let ref_end = ref_addr + std::mem::size_of::<T>();
        ref_addr >= buf_start && ref_end <= buf_end
    }

    // SystemEvent
    fn make_system_event(event_code: u8) -> [u8; 12] {
        let mut buf = [0u8; 12];
        buf[0] = b'S';
        write_u16_be(&mut buf, 1, 42);
        write_u16_be(&mut buf, 3, 7);
        write_u48_be(&mut buf, 5, 123_456_789);
        buf[11] = event_code;
        buf
    }

    struct SystemEventHandler {
        called: bool,
    }

    impl MessageHandler for SystemEventHandler {
        fn on_system_event(&mut self, msg: &SystemEvent) {
            self.called = true;
            assert_eq!(msg.message_type, b'S');
            assert_eq!(msg.stock_locate.get(), 42);
            assert_eq!(msg.tracking_number.get(), 7);
            assert_eq!(msg.timestamp.get(), 123_456_789);
            assert_eq!(msg.event_code, b'O');
        }
    }

    #[test]
    fn parse_system_event() {
        let buf = make_system_event(b'O');
        let mut handler = SystemEventHandler { called: false };
        parse_and_dispatch(&buf, &mut handler);
        assert!(handler.called, "on_system_event was not called");
    }

    struct SystemEventZeroCopyHandler {
        buf_ptr: *const u8,
        buf_len: usize,
    }

    impl MessageHandler for SystemEventZeroCopyHandler {
        fn on_system_event(&mut self, msg: &SystemEvent) {
            let buf_start = self.buf_ptr as usize;
            let buf_end = buf_start + self.buf_len;
            let ref_addr = std::ptr::from_ref::<SystemEvent>(msg) as usize;
            let ref_end = ref_addr + std::mem::size_of::<SystemEvent>();
            assert!(
                ref_addr >= buf_start && ref_end <= buf_end,
                "SystemEvent is not zero-copy"
            );
        }
    }

    #[test]
    fn system_event_zero_copy() {
        let buf = make_system_event(b'O');
        let mut handler = SystemEventZeroCopyHandler {
            buf_ptr: buf.as_ptr(),
            buf_len: buf.len(),
        };
        parse_and_dispatch(&buf, &mut handler);
    }

    struct NoopHandler;

    impl MessageHandler for NoopHandler {}

    #[test]
    fn unknown_message_type_is_ignored() {
        let buf = [0u8; 1];
        let mut handler = NoopHandler;
        parse_and_dispatch(&buf, &mut handler);
    }

    #[test]
    fn too_short_message_is_ignored() {
        let buf = [b'A'];
        let mut handler = NoopHandler;
        parse_and_dispatch(&buf, &mut handler);
    }

    // AddOrderNoMPID
    fn make_add_order_no_mpid() -> [u8; 36] {
        let mut buf = [0u8; 36];
        buf[0] = b'A';
        write_u16_be(&mut buf, 1, 100);
        write_u16_be(&mut buf, 3, 200);
        write_u48_be(&mut buf, 5, 999_888_777);
        write_u64_be(&mut buf, 11, 0xABCD_EF01_2345_6789);
        buf[19] = b'B';
        write_u32_be(&mut buf, 20, 500);
        write_u64_be(&mut buf, 24, u64::from_be_bytes(*b"AAPL    "));
        write_u32_be(&mut buf, 32, 15000);
        buf
    }

    struct AddOrderNoMPIDHandler {
        called: bool,
    }

    impl MessageHandler for AddOrderNoMPIDHandler {
        fn on_add_order_no_mpid(&mut self, msg: &AddOrderNoMPID) -> Option<SlabIndex> {
            self.called = true;
            assert_eq!(msg.message_type, b'A');
            assert_eq!(msg.stock_locate.get(), 100);
            assert_eq!(msg.tracking_number.get(), 200);
            assert_eq!(msg.timestamp.get(), 999_888_777);
            assert_eq!(msg.order_ref_number.get(), 0xABCD_EF01_2345_6789);
            assert_eq!(msg.indicator, b'B');
            assert_eq!(msg.shares.get(), 500);
            assert_eq!(msg.stock.get(), u64::from_be_bytes(*b"AAPL    "));
            assert_eq!(msg.price.get(), 15000);
        None
        }
    }

    #[test]
    fn parse_add_order_no_mpid() {
        let buf = make_add_order_no_mpid();
        let mut handler = AddOrderNoMPIDHandler { called: false };
        parse_and_dispatch(&buf, &mut handler);
        assert!(handler.called, "on_add_order_no_mpid was not called");
    }

    #[test]
    fn add_order_no_mpid_zero_copy() {
        struct Handler {
            buf_ptr: *const u8,
            buf_len: usize,
        }
        impl MessageHandler for Handler {
            fn on_add_order_no_mpid(&mut self, msg: &AddOrderNoMPID) -> Option<SlabIndex> {
                assert!(is_zero_copy(
                    unsafe { std::slice::from_raw_parts(self.buf_ptr, self.buf_len) },
                    msg
                ));
                None
            }
        }
        let buf = make_add_order_no_mpid();
        let mut handler = Handler {
            buf_ptr: buf.as_ptr(),
            buf_len: buf.len(),
        };
        parse_and_dispatch(&buf, &mut handler);
    }

    //AddOrderWithMPID
    fn make_add_order_with_mpid() -> [u8; 40] {
        let mut buf = [0u8; 40];
        buf[0] = b'F';
        write_u16_be(&mut buf, 1, 101);
        write_u16_be(&mut buf, 3, 201);
        write_u48_be(&mut buf, 5, 111_222_333);
        write_u64_be(&mut buf, 11, 0xDEAD_BEEF_CAFE_BABE);
        buf[19] = b'S';
        write_u32_be(&mut buf, 20, 300);
        write_u64_be(&mut buf, 24, u64::from_be_bytes(*b"GOOGL   "));
        write_u32_be(&mut buf, 32, 100_000);
        write_u32_be(&mut buf, 36, 9999);
        buf
    }

    struct AddOrderWithMPIDHandler {
        called: bool,
    }

    impl MessageHandler for AddOrderWithMPIDHandler {
        fn on_add_order_with_mpid(&mut self, msg: &AddOrderWithMPID) -> Option<SlabIndex> {
            self.called = true;
            assert_eq!(msg.message_type, b'F');
            assert_eq!(msg.stock_locate.get(), 101);
            assert_eq!(msg.tracking_number.get(), 201);
            assert_eq!(msg.timestamp.get(), 111_222_333);
            assert_eq!(msg.order_ref_number.get(), 0xDEAD_BEEF_CAFE_BABE);
            assert_eq!(msg.indicator, b'S');
            assert_eq!(msg.shares.get(), 300);
            assert_eq!(msg.stock.get(), u64::from_be_bytes(*b"GOOGL   "));
            assert_eq!(msg.price.get(), 100_000);
            assert_eq!(msg.attribution.get(), 9999);
        None
        }
    }

    #[test]
    fn parse_add_order_with_mpid() {
        let buf = make_add_order_with_mpid();
        let mut handler = AddOrderWithMPIDHandler { called: false };
        parse_and_dispatch(&buf, &mut handler);
        assert!(handler.called, "on_add_order_with_mpid was not called");
    }

    #[test]
    fn add_order_with_mpid_zero_copy() {
        struct Handler {
            buf_ptr: *const u8,
            buf_len: usize,
        }
        impl MessageHandler for Handler {
            fn on_add_order_with_mpid(&mut self, msg: &AddOrderWithMPID) -> Option<SlabIndex> {
                assert!(is_zero_copy(
                    unsafe { std::slice::from_raw_parts(self.buf_ptr, self.buf_len) },
                    msg
                ));
                None
            }
        }
        let buf = make_add_order_with_mpid();
        let mut handler = Handler {
            buf_ptr: buf.as_ptr(),
            buf_len: buf.len(),
        };
        parse_and_dispatch(&buf, &mut handler);
    }

    // OrderCancel
    fn make_order_cancel() -> [u8; 23] {
        let mut buf = [0u8; 23];
        buf[0] = b'X';
        write_u16_be(&mut buf, 1, 55);
        write_u16_be(&mut buf, 3, 66);
        write_u48_be(&mut buf, 5, 444_555_666);
        write_u64_be(&mut buf, 11, 0x1111_2222_3333_4444);
        write_u32_be(&mut buf, 19, 150);
        buf
    }

    struct OrderCancelHandler {
        called: bool,
    }

    impl MessageHandler for OrderCancelHandler {
        fn on_order_cancel(&mut self, msg: &OrderCancel) {
            self.called = true;
            assert_eq!(msg.message_type, b'X');
            assert_eq!(msg.stock_locate.get(), 55);
            assert_eq!(msg.tracking_number.get(), 66);
            assert_eq!(msg.timestamp.get(), 444_555_666);
            assert_eq!(msg.order_ref_number.get(), 0x1111_2222_3333_4444);
            assert_eq!(msg.cancelled_shares.get(), 150);
        }
    }

    #[test]
    fn parse_order_cancel() {
        let buf = make_order_cancel();
        let mut handler = OrderCancelHandler { called: false };
        parse_and_dispatch(&buf, &mut handler);
        assert!(handler.called, "on_order_cancel was not called");
    }

    #[test]
    fn order_cancel_zero_copy() {
        struct Handler {
            buf_ptr: *const u8,
            buf_len: usize,
        }
        impl MessageHandler for Handler {
            fn on_order_cancel(&mut self, msg: &OrderCancel) {
                assert!(is_zero_copy(
                    unsafe { std::slice::from_raw_parts(self.buf_ptr, self.buf_len) },
                    msg
                ));
            }
        }
        let buf = make_order_cancel();
        let mut handler = Handler {
            buf_ptr: buf.as_ptr(),
            buf_len: buf.len(),
        };
        parse_and_dispatch(&buf, &mut handler);
    }

    // OrderDelete
    fn make_order_delete() -> [u8; 19] {
        let mut buf = [0u8; 19];
        buf[0] = b'D';
        write_u16_be(&mut buf, 1, 77);
        write_u16_be(&mut buf, 3, 88);
        write_u48_be(&mut buf, 5, 777_888_999);
        write_u64_be(&mut buf, 11, 0xAAAA_BBBB_CCCC_DDDD);
        buf
    }

    struct OrderDeleteHandler {
        called: bool,
    }

    impl MessageHandler for OrderDeleteHandler {
        fn on_order_delete(&mut self, msg: &OrderDelete) {
            self.called = true;
            assert_eq!(msg.message_type, b'D');
            assert_eq!(msg.stock_locate.get(), 77);
            assert_eq!(msg.tracking_number.get(), 88);
            assert_eq!(msg.timestamp.get(), 777_888_999);
            assert_eq!(msg.order_ref_number.get(), 0xAAAA_BBBB_CCCC_DDDD);
        }
    }

    #[test]
    fn parse_order_delete() {
        let buf = make_order_delete();
        let mut handler = OrderDeleteHandler { called: false };
        parse_and_dispatch(&buf, &mut handler);
        assert!(handler.called, "on_order_delete was not called");
    }

    #[test]
    fn order_delete_zero_copy() {
        struct Handler {
            buf_ptr: *const u8,
            buf_len: usize,
        }
        impl MessageHandler for Handler {
            fn on_order_delete(&mut self, msg: &OrderDelete) {
                assert!(is_zero_copy(
                    unsafe { std::slice::from_raw_parts(self.buf_ptr, self.buf_len) },
                    msg
                ));
            }
        }
        let buf = make_order_delete();
        let mut handler = Handler {
            buf_ptr: buf.as_ptr(),
            buf_len: buf.len(),
        };
        parse_and_dispatch(&buf, &mut handler);
    }

    // OrderReplace
    fn make_order_replace() -> [u8; 35] {
        let mut buf = [0u8; 35];
        buf[0] = b'U';
        write_u16_be(&mut buf, 1, 10);
        write_u16_be(&mut buf, 3, 20);
        write_u48_be(&mut buf, 5, 100_200_300);
        write_u64_be(&mut buf, 11, 0x0000_1111_2222_3333);
        write_u64_be(&mut buf, 19, 0x4444_5555_6666_7777);
        write_u32_be(&mut buf, 27, 800);
        write_u32_be(&mut buf, 31, 25000);
        buf
    }

    struct OrderReplaceHandler {
        called: bool,
    }

    impl MessageHandler for OrderReplaceHandler {
        fn on_order_replace(&mut self, msg: &OrderReplace) {
            self.called = true;
            assert_eq!(msg.message_type, b'U');
            assert_eq!(msg.stock_locate.get(), 10);
            assert_eq!(msg.tracking_number.get(), 20);
            assert_eq!(msg.timestamp.get(), 100_200_300);
            assert_eq!(msg.org_order_ref_number.get(), 0x0000_1111_2222_3333);
            assert_eq!(msg.new_order_ref_number.get(), 0x4444_5555_6666_7777);
            assert_eq!(msg.shares.get(), 800);
            assert_eq!(msg.price.get(), 25_000);
        }
    }

    #[test]
    fn parse_order_replace() {
        let buf = make_order_replace();
        let mut handler = OrderReplaceHandler { called: false };
        parse_and_dispatch(&buf, &mut handler);
        assert!(handler.called, "on_order_replace was not called");
    }

    #[test]
    fn order_replace_zero_copy() {
        struct Handler {
            buf_ptr: *const u8,
            buf_len: usize,
        }
        impl MessageHandler for Handler {
            fn on_order_replace(&mut self, msg: &OrderReplace) {
                assert!(is_zero_copy(
                    unsafe { std::slice::from_raw_parts(self.buf_ptr, self.buf_len) },
                    msg
                ));
            }
        }
        let buf = make_order_replace();
        let mut handler = Handler {
            buf_ptr: buf.as_ptr(),
            buf_len: buf.len(),
        };
        parse_and_dispatch(&buf, &mut handler);
    }

    // OrderExecute
    fn make_order_execute() -> [u8; 31] {
        let mut buf = [0u8; 31];
        buf[0] = b'E';
        write_u16_be(&mut buf, 1, 30);
        write_u16_be(&mut buf, 3, 40);
        write_u48_be(&mut buf, 5, 555_666_777);
        write_u64_be(&mut buf, 11, 0x8888_9999_AAAA_BBBB);
        write_u32_be(&mut buf, 19, 250);
        write_u64_be(&mut buf, 23, 0xCCCC_DDDD_EEEE_FFFF);
        buf
    }

    struct OrderExecuteHandler {
        called: bool,
    }

    impl MessageHandler for OrderExecuteHandler {
        fn on_order_execute(&mut self, msg: &OrderExecute) {
            self.called = true;
            assert_eq!(msg.message_type, b'E');
            assert_eq!(msg.stock_locate.get(), 30);
            assert_eq!(msg.tracking_number.get(), 40);
            assert_eq!(msg.timestamp.get(), 555_666_777);
            assert_eq!(msg.order_ref_number.get(), 0x8888_9999_AAAA_BBBB);
            assert_eq!(msg.executed_shares.get(), 250);
            assert_eq!(msg.match_number.get(), 0xCCCC_DDDD_EEEE_FFFF);
        }
    }

    #[test]
    fn parse_order_execute() {
        let buf = make_order_execute();
        let mut handler = OrderExecuteHandler { called: false };
        parse_and_dispatch(&buf, &mut handler);
        assert!(handler.called, "on_order_execute was not called");
    }

    #[test]
    fn order_execute_zero_copy() {
        struct Handler {
            buf_ptr: *const u8,
            buf_len: usize,
        }
        impl MessageHandler for Handler {
            fn on_order_execute(&mut self, msg: &OrderExecute) {
                assert!(is_zero_copy(
                    unsafe { std::slice::from_raw_parts(self.buf_ptr, self.buf_len) },
                    msg
                ));
            }
        }
        let buf = make_order_execute();
        let mut handler = Handler {
            buf_ptr: buf.as_ptr(),
            buf_len: buf.len(),
        };
        parse_and_dispatch(&buf, &mut handler);
    }

    // OrderExecuteWithPrice
    fn make_order_execute_with_price() -> [u8; 36] {
        let mut buf = [0u8; 36];
        buf[0] = b'C';
        write_u16_be(&mut buf, 1, 50);
        write_u16_be(&mut buf, 3, 60);
        write_u48_be(&mut buf, 5, 333_444_555);
        write_u64_be(&mut buf, 11, 0x1111_AAAA_2222_BBBB);
        write_u32_be(&mut buf, 19, 1000);
        write_u64_be(&mut buf, 23, 0x3333_CCCC_4444_DDDD);
        buf[31] = b'Y';
        write_u32_be(&mut buf, 32, 75000);
        buf
    }

    struct OrderExecuteWithPriceHandler {
        called: bool,
    }

    impl MessageHandler for OrderExecuteWithPriceHandler {
        fn on_order_execute_with_price(&mut self, msg: &OrderExecuteWithPrice) {
            self.called = true;
            assert_eq!(msg.message_type, b'C');
            assert_eq!(msg.stock_locate.get(), 50);
            assert_eq!(msg.tracking_number.get(), 60);
            assert_eq!(msg.timestamp.get(), 333_444_555);
            assert_eq!(msg.order_ref_number.get(), 0x1111_AAAA_2222_BBBB);
            assert_eq!(msg.executed_shares.get(), 1000);
            assert_eq!(msg.match_number.get(), 0x3333_CCCC_4444_DDDD);
            assert_eq!(msg.printable, b'Y');
            assert_eq!(msg.execution_price.get(), 75_000);
        }
    }

    #[test]
    fn parse_order_execute_with_price() {
        let buf = make_order_execute_with_price();
        let mut handler = OrderExecuteWithPriceHandler { called: false };
        parse_and_dispatch(&buf, &mut handler);
        assert!(handler.called, "on_order_execute_with_price was not called");
    }

    #[test]
    fn order_execute_with_price_zero_copy() {
        struct Handler {
            buf_ptr: *const u8,
            buf_len: usize,
        }
        impl MessageHandler for Handler {
            fn on_order_execute_with_price(&mut self, msg: &OrderExecuteWithPrice) {
                assert!(is_zero_copy(
                    unsafe { std::slice::from_raw_parts(self.buf_ptr, self.buf_len) },
                    msg
                ));
            }
        }
        let buf = make_order_execute_with_price();
        let mut handler = Handler {
            buf_ptr: buf.as_ptr(),
            buf_len: buf.len(),
        };
        parse_and_dispatch(&buf, &mut handler);
    }
}
