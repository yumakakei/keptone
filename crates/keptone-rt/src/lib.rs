// SPDX-FileCopyrightText: 2026 Yuma Kakei <yumasansansan@gmail.com>
// SPDX-License-Identifier: GPL-3.0-or-later
//
// This file is part of Keptone. The SPDX lines name its copyright holder and its
// license, in the machine-readable form of the REUSE specification: the GNU
// General Public License, version 3 or any later version
// (LICENSES/GPL-3.0-or-later.txt).

//! The wait-free queues between the audio thread and the others.
//!
//! A bounded single-producer single-consumer ring, a triple-buffer mailbox in
//! which the latest value wins, and a queue that carries retired objects back to
//! the main thread to be freed there (docs/DESIGN.md §5). Ownership only moves
//! through them; nothing is shared and mutable. This is the only lock-free code
//! of Keptone. Its unsafe blocks, atomics and `UnsafeCell`, each hold one
//! operation and say why it is sound, and the queues are tested under Miri and
//! loom.
//!
//! Nothing is here yet: the crate is filled in milestone M5 (docs/DESIGN.md
//! §16).
