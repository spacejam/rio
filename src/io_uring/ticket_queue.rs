use super::*;

/// Exerts backpressure on submission threads
/// to ensure that there are never more submissions
/// in-flight than available slots in the completion
/// queue. Normally io_uring would accept the excess,
/// and just drop the overflowing completions.
#[derive(Debug)]
pub(crate) struct TicketQueue {
    tickets: Mutex<Vec<usize>>,
    cv: Condvar,
}

impl TicketQueue {
    pub(crate) fn new(size: usize) -> TicketQueue {
        let tickets = Mutex::new((0..size).collect());
        TicketQueue {
            tickets,
            cv: Condvar::new(),
        }
    }

    pub(crate) fn push_multi(
        &self,
        mut new_tickets: Vec<usize>,
    ) {
        let _ = Measure::new(&M.ticket_queue_push);
        let mut tickets = self.tickets.lock().unwrap();
        tickets.append(&mut new_tickets);
        self.cv.notify_one();
    }

    pub(crate) fn pop(&self) -> usize {
        let _ = Measure::new(&M.ticket_queue_pop);
        let mut tickets = self.tickets.lock().unwrap();
        while tickets.is_empty() {
            tickets = self.cv.wait(tickets).unwrap();
        }
        tickets.pop().unwrap()
    }
}
