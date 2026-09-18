#![cfg(feature = "kotlin")]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, owners};

const KOTLIN_SOURCE: &str = r#"
package com.example

import kotlin.collections.List
import com.example.ApiError as Err

class ApiError(val message: String) : RuntimeException(), Failing {
    val code: Int = 400
    var seen = false
    lateinit var service: Formatter

    fun render(): String = Formatter.format(message)

    override fun fail(): ApiError = this

    companion object {
        const val PREFIX = "api"
        fun badRequest(message: String): ApiError = ApiError(message)
    }
}

object Registry {
    fun lookup(key: String): ApiError? = null
}

interface Failing {
    val tag: String
    val label: String get() = "x"
    fun fail(): ApiError
    fun describe(): String = "failed"
}

fun interface Handler {
    fun handle(e: ApiError)
}

abstract class Base {
    abstract fun run()
    abstract val slot: Int
}

enum class Level {
    LOW, HIGH;
    fun lower(): String = name
}

data class Point(val x: Int, val y: Int)

typealias Errors = List<ApiError>

const val TOP = 1
val topVar: Int = 2

fun Formatter.shout(text: String): String = text
val Formatter.badge: String get() = "x"
fun <T> List<T>.second(): T = this[1]

fun main() {
    val scratch = ApiError("x")
    val runner = object : Handler {
        override fun handle(err: ApiError) {}
    }
    scratch.render()
    Registry.lookup("k")
    println(scratch.code)
}
"#;

#[test]
fn kotlin_types_members_and_references() {
    let tags = tag_source("src/main/kotlin/ApiError.kt", KOTLIN_SOURCE);

    // A `class` (plain, `data`, `enum`) and an `object` are classes; an
    // `interface` - including a `fun interface` - is an interface.
    let api_error = find(&tags, "ApiError", OccKind::Definition);
    assert_eq!(api_error.len(), 1);
    assert_eq!(api_error[0].sym_kind, SymKind::Class);
    assert!(owners(api_error[0]).is_empty());

    let failing = find(&tags, "Failing", OccKind::Definition);
    assert_eq!(failing.len(), 1);
    assert_eq!(failing[0].sym_kind, SymKind::Interface);

    let handler = find(&tags, "Handler", OccKind::Definition);
    assert_eq!(handler.len(), 1);
    assert_eq!(
        handler[0].sym_kind,
        SymKind::Interface,
        "`fun interface` is still an interface"
    );

    let registry = find(&tags, "Registry", OccKind::Definition);
    assert_eq!(registry.len(), 1);
    assert_eq!(registry[0].sym_kind, SymKind::Class);

    let level = find(&tags, "Level", OccKind::Definition);
    assert_eq!(level.len(), 1);
    assert_eq!(
        level[0].sym_kind,
        SymKind::Class,
        "an enum class is a class"
    );

    let point = find(&tags, "Point", OccKind::Definition);
    assert_eq!(point.len(), 1);
    assert_eq!(point[0].sym_kind, SymKind::Class);

    let errors = find(&tags, "Errors", OccKind::Definition);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].sym_kind, SymKind::Type);

    // A member function of a concrete type is a method owned by it; a
    // top-level function is a plain function with no owner.
    let render = find(&tags, "render", OccKind::Definition);
    assert_eq!(render.len(), 1);
    assert_eq!(render[0].sym_kind, SymKind::Method);
    assert_eq!(owners(render[0]), vec!["ApiError"]);
    assert_eq!(render[0].context.as_deref(), Some("ApiError"));
    assert_eq!(render[0].line, 12);

    let lookup_def = find(&tags, "lookup", OccKind::Definition);
    assert_eq!(lookup_def.len(), 1);
    assert_eq!(lookup_def[0].sym_kind, SymKind::Method);
    assert_eq!(owners(lookup_def[0]), vec!["Registry"]);

    let lower = find(&tags, "lower", OccKind::Definition);
    assert_eq!(lower.len(), 1);
    assert_eq!(lower[0].sym_kind, SymKind::Method);
    assert_eq!(owners(lower[0]), vec!["Level"]);

    let main = find(&tags, "main", OccKind::Definition);
    assert_eq!(main.len(), 1);
    assert_eq!(main[0].sym_kind, SymKind::Function);
    assert!(owners(main[0]).is_empty());

    // A companion member belongs to the enclosing class: `ApiError.badRequest`
    // is how it is called.
    let bad_request = find(&tags, "badRequest", OccKind::Definition);
    assert_eq!(bad_request.len(), 1);
    assert_eq!(owners(bad_request[0]), vec!["ApiError"]);

    // Every interface member only declares - even one carrying a default body
    // or a getter. The concrete override defines.
    let fail_decl = find(&tags, "fail", OccKind::Declaration);
    assert_eq!(fail_decl.len(), 1);
    assert_eq!(fail_decl[0].sym_kind, SymKind::Method);
    assert_eq!(owners(fail_decl[0]), vec!["Failing"]);
    let fail_def = find(&tags, "fail", OccKind::Definition);
    assert_eq!(fail_def.len(), 1);
    assert_eq!(owners(fail_def[0]), vec!["ApiError"]);

    let describe = find(&tags, "describe", OccKind::Declaration);
    assert_eq!(describe.len(), 1);
    assert_eq!(owners(describe[0]), vec!["Failing"]);
    assert!(
        find(&tags, "describe", OccKind::Definition).is_empty(),
        "an interface member with a default body still only declares"
    );

    let label = find(&tags, "label", OccKind::Declaration);
    assert_eq!(label.len(), 1);
    assert_eq!(label[0].sym_kind, SymKind::Field);
    assert_eq!(owners(label[0]), vec!["Failing"]);
    assert!(
        find(&tags, "label", OccKind::Definition).is_empty(),
        "an interface property with a getter still only declares"
    );

    let tag_decl = find(&tags, "tag", OccKind::Declaration);
    assert_eq!(tag_decl.len(), 1);
    assert_eq!(tag_decl[0].sym_kind, SymKind::Field);
    assert_eq!(owners(tag_decl[0]), vec!["Failing"]);
    assert!(find(&tags, "tag", OccKind::Definition).is_empty());

    let handle_decl = find(&tags, "handle", OccKind::Declaration);
    assert_eq!(handle_decl.len(), 1);
    assert_eq!(owners(handle_decl[0]), vec!["Handler"]);
    let handle_def = find(&tags, "handle", OccKind::Definition);
    assert_eq!(
        handle_def.len(),
        1,
        "the override in an anonymous `object : Handler` defines"
    );
    assert_eq!(handle_def[0].sym_kind, SymKind::Method);

    // An `abstract` member of a class declares too.
    let run = find(&tags, "run", OccKind::Declaration);
    assert_eq!(run.len(), 1);
    assert_eq!(run[0].sym_kind, SymKind::Method);
    assert_eq!(owners(run[0]), vec!["Base"]);
    assert!(find(&tags, "run", OccKind::Definition).is_empty());

    let slot = find(&tags, "slot", OccKind::Declaration);
    assert_eq!(slot.len(), 1);
    assert_eq!(slot[0].sym_kind, SymKind::Field);
    assert_eq!(owners(slot[0]), vec!["Base"]);
    assert!(find(&tags, "slot", OccKind::Definition).is_empty());

    // Properties: member -> field, `const val` -> constant, top level ->
    // variable.
    let code_def = find(&tags, "code", OccKind::Definition);
    assert_eq!(code_def.len(), 1);
    assert_eq!(code_def[0].sym_kind, SymKind::Field);
    assert_eq!(owners(code_def[0]), vec!["ApiError"]);

    let seen = find(&tags, "seen", OccKind::Definition);
    assert_eq!(seen.len(), 1, "a `var` member is a field too");
    assert_eq!(seen[0].sym_kind, SymKind::Field);
    assert_eq!(owners(seen[0]), vec!["ApiError"]);

    let service = find(&tags, "service", OccKind::Definition);
    assert_eq!(
        service.len(),
        1,
        "`lateinit var` declares storage with no initialiser and no accessor"
    );
    assert_eq!(service[0].sym_kind, SymKind::Field);
    assert_eq!(owners(service[0]), vec!["ApiError"]);
    assert!(find(&tags, "service", OccKind::Declaration).is_empty());

    let message = find(&tags, "message", OccKind::Definition);
    assert_eq!(
        message.len(),
        1,
        "`val` in a primary constructor is a field"
    );
    assert_eq!(message[0].sym_kind, SymKind::Field);
    assert_eq!(owners(message[0]), vec!["ApiError"]);

    let x = find(&tags, "x", OccKind::Definition);
    assert_eq!(x.len(), 1);
    assert_eq!(owners(x[0]), vec!["Point"]);

    let prefix = find(&tags, "PREFIX", OccKind::Definition);
    assert_eq!(prefix.len(), 1);
    assert_eq!(prefix[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(prefix[0]), vec!["ApiError"]);

    let top = find(&tags, "TOP", OccKind::Definition);
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].sym_kind, SymKind::Constant);
    assert!(owners(top[0]).is_empty());

    let top_var = find(&tags, "topVar", OccKind::Definition);
    assert_eq!(top_var.len(), 1);
    assert_eq!(top_var[0].sym_kind, SymKind::Variable);

    let low = find(&tags, "LOW", OccKind::Definition);
    assert_eq!(low.len(), 1);
    assert_eq!(low[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(low[0]), vec!["Level"]);

    // A local `val` inside a function body is not worth indexing.
    for local in ["scratch", "runner", "err", "key"] {
        assert!(
            !tags.iter().any(|tag| &*tag.name == local),
            "`{local}` is a local or a parameter and stays untagged"
        );
    }

    // Calls: an UpperCamelCase receiver names the owner, a lowercase one is a
    // variable and claims none.
    let format = find(&tags, "format", OccKind::Call);
    assert_eq!(format.len(), 1);
    assert_eq!(owners(format[0]), vec!["Formatter"]);

    let lookup_call = find(&tags, "lookup", OccKind::Call);
    assert_eq!(lookup_call.len(), 1);
    assert_eq!(owners(lookup_call[0]), vec!["Registry"]);

    let render_call = find(&tags, "render", OccKind::Call);
    assert_eq!(render_call.len(), 1);
    assert!(
        owners(render_call[0]).is_empty(),
        "a lowercase receiver is a local, not an owner"
    );

    let println = find(&tags, "println", OccKind::Call);
    assert_eq!(println.len(), 1);
    assert!(owners(println[0]).is_empty());

    let ctor = find(&tags, "ApiError", OccKind::Call);
    assert_eq!(ctor.len(), 2, "both constructor invocations are calls");

    let code_ref = find(&tags, "code", OccKind::Field);
    assert_eq!(code_ref.len(), 1);
    assert!(owners(code_ref[0]).is_empty());

    // Supertype lists - bare, constructor-invoking, and on an anonymous
    // object - are implementations.
    let impls: Vec<&str> = tags
        .iter()
        .filter(|tag| tag.kind == OccKind::Impl)
        .map(|tag| &*tag.name)
        .collect();
    assert!(impls.contains(&"Failing"), "got {impls:?}");
    assert!(impls.contains(&"RuntimeException"), "got {impls:?}");
    assert!(impls.contains(&"Handler"), "got {impls:?}");

    // `import kotlin.collections.List` references `List`; the lowercase
    // package segments are not symbols. An `as` alias is its own import.
    let import = find(&tags, "List", OccKind::Import);
    assert_eq!(import.len(), 1);
    assert_eq!(import[0].line, 4);
    assert!(owners(import[0]).is_empty());
    assert!(!tags.iter().any(|tag| &*tag.name == "collections"));

    let alias = find(&tags, "Err", OccKind::Import);
    assert_eq!(alias.len(), 1);
    assert_eq!(alias[0].line, 5);

    // Types named in signatures are type references.
    let string_refs = find(&tags, "String", OccKind::Type);
    assert!(
        string_refs.len() >= 5,
        "parameter, return and property types: got {}",
        string_refs.len()
    );

    // An extension belongs to the type it extends, at the top level and
    // through a generic receiver.
    let shout = find(&tags, "shout", OccKind::Definition);
    assert_eq!(shout.len(), 1);
    assert_eq!(
        owners(shout[0]),
        vec!["Formatter"],
        "`fun Formatter.shout()` extends Formatter"
    );

    let badge = find(&tags, "badge", OccKind::Definition);
    assert_eq!(badge.len(), 1);
    assert_eq!(
        owners(badge[0]),
        vec!["Formatter"],
        "`val Formatter.badge` extends Formatter"
    );

    let second = find(&tags, "second", OccKind::Definition);
    assert_eq!(second.len(), 1);
    assert_eq!(
        owners(second[0]),
        vec!["List"],
        "a generic receiver answers as its base type"
    );
}
