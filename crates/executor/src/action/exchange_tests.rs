use super::{ExchangeCursor, Fed, Item, Unmet};

fn met(item: usize, pairs: &[usize]) -> Fed {
    Fed::Met { item, pairs: pairs.to_vec() }
}

#[test]
fn sends_come_out_in_order_until_a_receive_waits() {
    let mut cursor = ExchangeCursor::new(vec![Item::Send, Item::Send, Item::Receive(1), Item::Send]);
    assert_eq!((cursor.next_send(), cursor.next_send(), cursor.next_send()), (Some(0), Some(1), None));
    assert_eq!(cursor.receiving(), Some(2));
    assert_eq!(cursor.feed(&[0]), met(2, &[0]));
    assert_eq!(cursor.receiving(), None);
    assert!(!cursor.is_done(), "the last send is still to go");
    assert_eq!((cursor.next_send(), cursor.next_send()), (Some(3), None));
    assert!(cursor.is_done());
}

#[test]
fn a_receive_skips_messages_that_fit_nothing_and_counts_them() {
    let mut cursor = ExchangeCursor::new(vec![Item::Receive(1)]);
    assert_eq!(cursor.next_send(), None);
    assert_eq!(cursor.feed(&[]), Fed::Skipped);
    assert_eq!(cursor.feed(&[]), Fed::Skipped);
    assert_eq!(cursor.feed(&[0]), met(0, &[2]), "the third message met it");
    assert!(cursor.is_done());
    assert_eq!(cursor.feed(&[0]), Fed::Skipped, "nothing waits once the exchange is done");
}

#[test]
fn a_group_pairs_its_entries_in_any_order() {
    let mut cursor = ExchangeCursor::new(vec![Item::Receive(2)]);
    assert_eq!(cursor.feed(&[1]), Fed::Held { item: 0 });
    assert_eq!(cursor.feed(&[0]), met(0, &[1, 0]), "pairs follow the written entries");
}

#[test]
fn a_group_moves_an_earlier_pair_to_make_room() {
    let mut cursor = ExchangeCursor::new(vec![Item::Receive(2)]);
    assert_eq!(cursor.feed(&[0, 1]), Fed::Held { item: 0 });
    assert_eq!(cursor.feed(&[0]), met(0, &[1, 0]));
}

#[test]
fn one_message_pairs_with_one_entry_only() {
    let mut cursor = ExchangeCursor::new(vec![Item::Receive(2)]);
    assert_eq!(cursor.feed(&[0, 1]), Fed::Held { item: 0 });
    assert_eq!(cursor.feed(&[0, 1]), met(0, &[0, 1]));
}

#[test]
fn an_entry_keeps_no_more_messages_than_the_group_has_entries() {
    let mut cursor = ExchangeCursor::new(vec![Item::Receive(2)]);
    assert_eq!(cursor.feed(&[0]), Fed::Held { item: 0 });
    assert_eq!(cursor.feed(&[0]), Fed::Held { item: 0 });
    assert_eq!(cursor.feed(&[0]), Fed::Skipped, "entry 0 can always take one of the two it holds");
    assert_eq!(cursor.feed(&[0, 1]), met(0, &[0, 3]), "a message is kept while one entry it fits has room");
}

#[test]
fn entries_the_group_lacks_are_ignored() {
    let mut cursor = ExchangeCursor::new(vec![Item::Receive(1)]);
    assert_eq!(cursor.feed(&[1, 5]), Fed::Skipped);
    assert_eq!(cursor.feed(&[5, 0]), met(0, &[1]));
}

#[test]
fn the_unmet_receive_names_its_open_entries_and_the_messages_since_the_last_match() {
    let mut cursor = ExchangeCursor::new(vec![Item::Receive(1), Item::Receive(2)]);
    assert_eq!(cursor.unmet(), Some(Unmet { item: 0, entries: vec![0], since: 0 }));
    assert_eq!(cursor.feed(&[0]), met(0, &[0]));
    assert_eq!(cursor.feed(&[]), Fed::Skipped);
    assert_eq!(cursor.feed(&[1]), Fed::Held { item: 1 });
    assert_eq!(cursor.unmet(), Some(Unmet { item: 1, entries: vec![0], since: 1 }));
    assert_eq!(cursor.feed(&[0]), met(1, &[3, 2]));
    assert_eq!(cursor.unmet(), None);
}

#[test]
fn an_exchange_without_a_receive_reads_to_the_end() {
    assert!(ExchangeCursor::new(vec![Item::Send]).reads_to_end());
    assert!(ExchangeCursor::new(Vec::new()).reads_to_end());
    assert!(ExchangeCursor::new(Vec::new()).is_done());
    assert!(!ExchangeCursor::new(vec![Item::Send, Item::Receive(1)]).reads_to_end());
    let empty_groups = ExchangeCursor::new(vec![Item::Receive(0), Item::Send, Item::Receive(0)]);
    assert!(empty_groups.reads_to_end(), "an empty group waits for nothing, so it is no receive");
    assert_eq!(ExchangeCursor::new(vec![Item::Send]).unmet(), None, "a send is not a receive");
}

#[test]
fn an_empty_group_is_passed_over() {
    let mut cursor = ExchangeCursor::new(vec![Item::Receive(0), Item::Send, Item::Receive(0)]);
    assert_eq!(cursor.receiving(), None);
    assert_eq!(cursor.next_send(), Some(1));
    assert!(cursor.is_done());
}
