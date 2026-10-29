//! N-replica convergence through a host relay, with delayed delivery.
//!
//! A host and three guests edit concurrently. Guests send to the host
//! (`apply_guest_update`); the host relays to every other guest
//! (`apply_host_update`). Delivery is a per-link FIFO drained at random
//! moments, so edits really are concurrent. At the end everything is flushed;
//! every replica's CRDT text must be identical, and so must each replica's
//! plain buffer, which is maintained *only* by replaying `BufferEdit`s, the
//! way the editor bridge does.

use proptest::prelude::*;
use std::collections::VecDeque;
use vix_cowork::{BufferEdit, DEFAULT_CAP_BYTES, Peer, PeerId, Permission, SharedDoc};

const GUESTS: usize = 3;

struct Node {
    doc: SharedDoc,
    buf: Vec<char>,
}

impl Node {
    fn replay(&mut self, edits: &[BufferEdit]) {
        for e in edits {
            self.buf
                .splice(e.offset..e.offset + e.delete, e.insert.chars());
        }
    }

    fn local_edit(
        &mut self,
        off: usize,
        del: usize,
        ins: &str,
    ) -> Result<Option<Vec<u8>>, vix_cowork::CoworkError> {
        let len = self.buf.len();
        let off = off % (len + 1);
        let del = del.min(len - off);
        self.buf.splice(off..off + del, ins.chars());
        self.doc.edit(off, del, ins)
    }
}

#[derive(Debug, Clone)]
enum Step {
    Edit {
        who: usize,
        off: usize,
        del: usize,
        ins: String,
    },
    Deliver {
        link: usize,
    },
    Undo {
        who: usize,
    },
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        6 => (0..=GUESTS, 0usize..64, 0usize..4, "[a-z é😀]{0,4}")
            .prop_map(|(who, off, del, ins)| Step::Edit { who, off, del, ins }),
        6 => (0..2 * GUESTS).prop_map(|link| Step::Deliver { link }),
        1 => (0..=GUESTS).prop_map(|who| Step::Undo { who }),
    ]
}

const GUEST_IDS: [u64; GUESTS] = [0, 1, 2];

fn editor(i: usize) -> Peer {
    let mut p = Peer::new(PeerId::guest(GUEST_IDS[i]), "g");
    p.permission = Permission::Edit;
    p
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn replicas_converge(steps in proptest::collection::vec(step(), 1..60)) {
        let host_doc = SharedDoc::host(PeerId::HOST, "seed text\nsecond line\n", DEFAULT_CAP_BYTES).unwrap();
        let snap = host_doc.snapshot().unwrap();
        let seed: Vec<char> = host_doc.text().chars().collect();
        let mut nodes = vec![Node { doc: host_doc, buf: seed.clone() }];
        for id in GUEST_IDS {
            let mut d = SharedDoc::from_snapshot(PeerId::guest(id), &snap, DEFAULT_CAP_BYTES).unwrap();
            d.set_undo_merge_ms(0);
            nodes.push(Node { doc: d, buf: seed.clone() });
        }
        nodes[0].doc.set_undo_merge_ms(0);
        // links 0..GUESTS: guest i -> host; GUESTS..2*GUESTS: host -> guest i.
        // Each host-bound queue holds (update); each guest-bound holds the update to apply.
        let mut up: Vec<VecDeque<Vec<u8>>> = vec![VecDeque::new(); GUESTS];
        let mut down: Vec<VecDeque<Vec<u8>>> = vec![VecDeque::new(); GUESTS];

        // Host sends its own update to all guests; relays guest updates to the others.
        let mut run = |steps: &[Step], flush: bool| {
            let apply_up = |nodes: &mut Vec<Node>, up: &mut Vec<VecDeque<Vec<u8>>>, down: &mut Vec<VecDeque<Vec<u8>>>, g: usize| {
                if let Some(u) = up[g].pop_front() {
                    let edits = nodes[0].doc.apply_guest_update(&editor(g), &u).unwrap();
                    nodes[0].replay(&edits);
                    for (j, q) in down.iter_mut().enumerate() {
                        if j != g { q.push_back(u.clone()); }
                    }
                    true
                } else { false }
            };
            let apply_down = |nodes: &mut Vec<Node>, down: &mut Vec<VecDeque<Vec<u8>>>, g: usize| {
                if let Some(u) = down[g].pop_front() {
                    let edits = nodes[g + 1].doc.apply_host_update(&u).unwrap();
                    nodes[g + 1].replay(&edits);
                    true
                } else { false }
            };
            for s in steps {
                match s {
                    Step::Edit { who, off, del, ins } => {
                        if let Some(u) = nodes[*who].local_edit(*off, *del, ins).unwrap() {
                            if *who == 0 { for q in &mut down { q.push_back(u.clone()); } }
                            else { up[who - 1].push_back(u); }
                        }
                    }
                    Step::Undo { who } => {
                        if let Some(r) = nodes[*who].doc.undo().unwrap() {
                            nodes[*who].replay(&r.edits);
                            if !r.update.is_empty() {
                                if *who == 0 { for q in &mut down { q.push_back(r.update.clone()); } }
                                else { up[who - 1].push_back(r.update); }
                            }
                        }
                    }
                    Step::Deliver { link } => {
                        if *link < GUESTS { apply_up(&mut nodes, &mut up, &mut down, *link); }
                        else { apply_down(&mut nodes, &mut down, link - GUESTS); }
                    }
                }
            }
            if flush {
                loop {
                    let mut moved = false;
                    for g in 0..GUESTS {
                        moved |= apply_up(&mut nodes, &mut up, &mut down, g);
                    }
                    for g in 0..GUESTS {
                        moved |= apply_down(&mut nodes, &mut down, g);
                    }
                    if !moved { break; }
                }
            }
        };
        run(&steps, true);

        let want = nodes[0].doc.text();
        for (i, n) in nodes.iter().enumerate() {
            prop_assert_eq!(&n.doc.text(), &want, "replica {} diverged", i);
            let buffer: String = n.buf.iter().collect();
            prop_assert_eq!(&buffer, &want, "buffer {} diverged from its doc", i);
        }
    }
}
