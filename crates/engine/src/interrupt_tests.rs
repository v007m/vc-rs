use std::time::Duration;

use super::interrupts;

#[tokio::test]
async fn an_abandon_is_the_third_interrupt_so_the_cleanups_are_abandoned_at_once() {
    let (interrupter, mut interrupts) = interrupts();
    interrupter.abandon();
    let abandoned = tokio::time::timeout(Duration::from_secs(1), interrupts.abandoned()).await;
    assert!(abandoned.is_ok(), "ready at once");
    assert_eq!(interrupts.seen(), 3);
}

#[tokio::test]
async fn an_abandon_after_a_stop_still_abandons_the_cleanups() {
    let (interrupter, mut interrupts) = interrupts();
    interrupter.interrupt();
    assert_eq!(interrupts.next().await, Some(()));
    interrupter.abandon();
    let abandoned = tokio::time::timeout(Duration::from_secs(1), interrupts.abandoned()).await;
    assert!(abandoned.is_ok());
    assert_eq!(interrupts.seen(), 3, "the rest wait unread");
}

#[test]
fn an_interrupter_knows_once_its_run_has_finished() {
    let (interrupter, interrupts) = interrupts();
    assert!(!interrupter.is_finished());
    drop(interrupts);
    assert!(interrupter.is_finished());
    interrupter.abandon();
}
