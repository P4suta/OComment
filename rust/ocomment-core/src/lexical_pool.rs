//! The alphabet the randomised property tests draw their sources from.
//!
//! Two suites generate sources this way — the checkpoint and incremental
//! properties in `src/incremental.rs`, and the whole-file properties in
//! `tests/properties.rs` — and they are meant to draw from the same alphabet: a
//! fragment worth generating against the whole-file scanner is worth generating
//! against the incremental one, because the incremental engine's promise is
//! that a restart reproduces what the whole-file scan would have said. One is a
//! unit test inside the crate and the other an integration test outside it, and
//! the only thing both can name is the crate's public surface, so the alphabet
//! lives here rather than being written out twice.
//!
//! It is `#[doc(hidden)]` and carries no stability promise: it is test support
//! that happens to have to be reachable from outside.

/// Single bytes that reach every built-in scanner's string, comment, here
/// document and template states rather than only the C-family delimiters.
///
/// A generator draws one of these against a much smaller weight of uniform
/// random bytes, so a delimiter arrives often enough for two of them to meet.
/// Each byte appears once and is drawn as often as the next; a caller that
/// wants one of them oftener says so with a weight of its own.
#[doc(hidden)]
pub const BYTES: &[u8] = b"\n\r/*'\"#`{}<>=[]-|?\\$%()@:!~";

/// Multi-byte tokens a single-byte alphabet can never synthesise.
///
/// The preamble and directive rules only fire on whole words, so without these
/// the generated sources never reach the code paths that make a scan depend on
/// where in the document it starts.
///
/// The two triple-quote runs are here for the opposite reason: they are three
/// of one byte, which a per-byte alphabet reaches only by coincidence, and they
/// open a string that swallows newlines in Python, Kotlin, Java, and TOML —
/// which is exactly the state a restart must not be allowed to land inside.
/// Lua's long brackets are the same state behind four bytes rather than three,
/// and the levelled forms are here because a closing bracket of the wrong level
/// is content: without them a generated source that opens one practically never
/// closes it. The eight YAML fragments after them are block scalar headers and
/// the indented line that follows one — a body is the state a YAML restart must
/// never land inside, and the bytes that open one have to arrive in that order.
/// Three of the eight put the owner of that body on an earlier line than its
/// header: a line ending in `:` or in a bare `-`, and the node properties that
/// may stand between the two. That owner is the one thing a YAML line does not
/// say about itself, so it is the one thing a restart at a line start has to be
/// refused over. The `|+` header is the keep-chomped body whose trailing blank
/// lines are content. The five PHP fragments after those are its two tags, an
/// attribute, and a here document header with the line that closes one: PHP
/// mode is the state a restart must never land inside, and only a whole `<?php`
/// opens it. The Ruby fragments at the end are its two column-zero markers with
/// the line each of them needs, a here document header with the terminator that
/// ends one, a percent literal opener, and the interpolation boundary a here
/// document header may be written across: four of Ruby's tokens are spelled
/// with a byte that is also an operator, so only the whole opener reaches the
/// state, and an embedded document, a here document body and the DATA section
/// are three more states a restart must never land inside. The body a header
/// inside `"#{ ... }"` asks for belongs to the line the header stands on rather
/// than to the interpolation, and only a pool that can assemble that opener
/// generates the case at all.
#[doc(hidden)]
pub const TOKENS: &[&[u8]] = &[
    b"coding:",
    b"# -*- coding: utf-8 -*-",
    b"# coding: latin-1",
    b"#!",
    b"//go:build",
    b"/*#__PURE__*/",
    b"<!--",
    b"r#\"",
    b"\"\"\"",
    b"'''",
    b"--[[",
    b"--[=[",
    b"]]",
    b"]=]",
    b": |\n",
    b"- >2\n",
    b"|+\n",
    b"\n  # ",
    b"k:\n",
    b"\n-\n",
    b"!!str ",
    b"&a ",
    b"<?php ",
    b"<?=",
    b"#[",
    b"<<<E\n",
    b"\nE;\n",
    b"=begin ",
    b"\n=end\n",
    b"__END__\n",
    b" <<~EOS\n",
    b"\nEOS\n",
    b"%w[",
    b"\"#{",
    b"}\"",
    b"\"#{ <<EOS }\"",
];
