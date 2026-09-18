#![cfg(feature = "scala")]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, owners};

const SCALA_SOURCE: &str = r#"
package com.example

import scala.collection.mutable.Buffer
import ApiError.badRequest

class ApiError(val message: String, code: Int) extends RuntimeException with Failing {
  val status: Int = 400
  type Payload = Map[String, String]

  def render(): String = Formatter.format(message)

  def fail(): ApiError = {
    val buf = Buffer.empty[String]
    buf.append(this.status.toString)
    this
  }
}

object ApiError {
  val Prefix = "api"

  def badRequest(message: String): ApiError = new ApiError(message, 400)
}

trait Failing {
  def fail(): ApiError
  val label: String
}

case class Wrapped(inner: ApiError) extends Failing

enum Level {
  case Low, High
}

def topLevel(x: Int): Int = x + 1

extension (e: ApiError) def summary: String = e.message

extension [T](xs: List[T]) def second: T = xs(1)

given ordApi: Ordering[ApiError] with {
  def compare(a: ApiError, b: ApiError): Int = 0
  val cache: Int = 0
}

package object util {
  def helper(): Int = 1
}

type Errors = List[ApiError]
"#;

#[test]
fn scala_templates_own_their_members() {
    let tags = tag_source("com/example/ApiError.scala", SCALA_SOURCE);

    // `package` is a module; `class`/`case class`/`object`/`enum` are classes,
    // `trait` is an interface.
    let pkg = find(&tags, "com.example", OccKind::Definition);
    assert_eq!(pkg.len(), 1);
    assert_eq!(pkg[0].sym_kind, SymKind::Module);

    let api_error = find(&tags, "ApiError", OccKind::Definition);
    assert_eq!(api_error.len(), 2, "the class and its companion object");
    assert!(
        api_error
            .iter()
            .all(|tag| tag.sym_kind == SymKind::Class && tag.owners.is_empty())
    );

    let failing = find(&tags, "Failing", OccKind::Definition);
    assert_eq!(failing.len(), 1);
    assert_eq!(failing[0].sym_kind, SymKind::Interface);

    let level = find(&tags, "Level", OccKind::Definition);
    assert_eq!(level.len(), 1);
    assert_eq!(level[0].sym_kind, SymKind::Class);

    let wrapped = find(&tags, "Wrapped", OccKind::Definition);
    assert_eq!(wrapped.len(), 1);
    assert_eq!(wrapped[0].sym_kind, SymKind::Class);

    // A `def` in a template is a method owned by it, and carries the template
    // as context; a top-level `def` is a plain function.
    let render = find(&tags, "render", OccKind::Definition);
    assert_eq!(render.len(), 1);
    assert_eq!(render[0].sym_kind, SymKind::Method);
    assert_eq!(owners(render[0]), vec!["ApiError"]);
    assert_eq!(render[0].context.as_deref(), Some("ApiError"));

    let bad_request = find(&tags, "badRequest", OccKind::Definition);
    assert_eq!(bad_request.len(), 1);
    assert_eq!(bad_request[0].sym_kind, SymKind::Method);
    assert_eq!(owners(bad_request[0]), vec!["ApiError"]);

    let top_level = find(&tags, "topLevel", OccKind::Definition);
    assert_eq!(top_level.len(), 1);
    assert_eq!(top_level[0].sym_kind, SymKind::Function);
    assert!(top_level[0].owners.is_empty());
    assert_eq!(top_level[0].context, None);

    // An extension method is owned by the type it extends.
    let summary = find(&tags, "summary", OccKind::Definition);
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].sym_kind, SymKind::Method);
    assert_eq!(owners(summary[0]), vec!["ApiError"]);

    let second = find(&tags, "second", OccKind::Definition);
    assert_eq!(second.len(), 1);
    assert_eq!(
        owners(second[0]),
        vec!["List"],
        "a generic receiver answers as its base type"
    );

    // A `given ... with { }` body holds real members.
    let compare = find(&tags, "compare", OccKind::Definition);
    assert_eq!(compare.len(), 1, "a given's with-body defines methods");
    assert_eq!(owners(compare[0]), vec!["ordApi"]);
    let cache = find(&tags, "cache", OccKind::Definition);
    assert_eq!(cache.len(), 1);
    assert_eq!(owners(cache[0]), vec!["ordApi"]);

    let helper = find(&tags, "helper", OccKind::Definition);
    assert_eq!(helper.len(), 1);
    assert_eq!(
        owners(helper[0]),
        vec!["util"],
        "a package object owns its members"
    );

    // A `def` in a trait declares without defining.
    let fail_def = find(&tags, "fail", OccKind::Definition);
    assert_eq!(fail_def.len(), 1);
    assert_eq!(owners(fail_def[0]), vec!["ApiError"]);
    let fail_decl = find(&tags, "fail", OccKind::Declaration);
    assert_eq!(fail_decl.len(), 1, "the trait member only declares");
    assert_eq!(fail_decl[0].sym_kind, SymKind::Method);
    assert_eq!(owners(fail_decl[0]), vec!["Failing"]);

    // `val`/`var` members, `val` class parameters and case-class parameters are
    // fields of their template; a `val` inside a method body is a local.
    let status = find(&tags, "status", OccKind::Definition);
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].sym_kind, SymKind::Field);
    assert_eq!(owners(status[0]), vec!["ApiError"]);

    let prefix = find(&tags, "Prefix", OccKind::Definition);
    assert_eq!(prefix.len(), 1);
    assert_eq!(prefix[0].sym_kind, SymKind::Field);
    assert_eq!(owners(prefix[0]), vec!["ApiError"]);

    let message = find(&tags, "message", OccKind::Definition);
    assert_eq!(message.len(), 1, "the `val` class parameter is a member");
    assert_eq!(message[0].sym_kind, SymKind::Field);
    assert_eq!(owners(message[0]), vec!["ApiError"]);

    let inner = find(&tags, "inner", OccKind::Definition);
    assert_eq!(inner.len(), 1, "every case-class parameter is a member");
    assert_eq!(owners(inner[0]), vec!["Wrapped"]);

    assert!(
        find(&tags, "code", OccKind::Definition).is_empty(),
        "a bare constructor parameter is not a member"
    );
    assert!(
        find(&tags, "buf", OccKind::Definition).is_empty(),
        "a local `val` is not tagged"
    );

    let label = find(&tags, "label", OccKind::Declaration);
    assert_eq!(label.len(), 1);
    assert_eq!(label[0].sym_kind, SymKind::Field);
    assert_eq!(owners(label[0]), vec!["Failing"]);

    // Enum cases are constants of the enum; `type` members are types.
    let low = find(&tags, "Low", OccKind::Definition);
    assert_eq!(low.len(), 1);
    assert_eq!(low[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(low[0]), vec!["Level"]);

    let payload = find(&tags, "Payload", OccKind::Definition);
    assert_eq!(payload.len(), 1);
    assert_eq!(payload[0].sym_kind, SymKind::Type);
    assert_eq!(owners(payload[0]), vec!["ApiError"]);

    let errors = find(&tags, "Errors", OccKind::Definition);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].sym_kind, SymKind::Type);

    // A call through an uppercase receiver claims that receiver as owner; a
    // lowercase one is a value and claims nothing. `this` resolves to the
    // enclosing template.
    let format = find(&tags, "format", OccKind::Call);
    assert_eq!(format.len(), 1);
    assert_eq!(owners(format[0]), vec!["Formatter"]);
    assert_eq!(format[0].context.as_deref(), Some("ApiError::render"));

    let append = find(&tags, "append", OccKind::Call);
    assert_eq!(append.len(), 1);
    assert!(
        append[0].owners.is_empty(),
        "`buf` is a local, not an owning type"
    );

    let status_use = find(&tags, "status", OccKind::Field);
    assert_eq!(status_use.len(), 1);
    assert_eq!(owners(status_use[0]), vec!["ApiError"], "`this.status`");

    // `new ApiError(...)` is a call of the type.
    let new_api_error = find(&tags, "ApiError", OccKind::Call);
    assert_eq!(new_api_error.len(), 1);
    assert_eq!(
        new_api_error[0].context.as_deref(),
        Some("ApiError::badRequest")
    );

    // `extends A with B` yields one impl reference per parent.
    let runtime = find(&tags, "RuntimeException", OccKind::Impl);
    assert_eq!(runtime.len(), 1);
    let failing_impl = find(&tags, "Failing", OccKind::Impl);
    assert_eq!(
        failing_impl.len(),
        2,
        "`with Failing` on the class and `extends Failing` on the case class"
    );
    assert_eq!(failing_impl[0].context.as_deref(), Some("ApiError"));

    // Imports: the last path segment, with an uppercase prefix as its owner.
    let buffer = find(&tags, "Buffer", OccKind::Import);
    assert_eq!(buffer.len(), 1);
    assert!(buffer[0].owners.is_empty(), "`mutable` is not a type");

    let imported = find(&tags, "badRequest", OccKind::Import);
    assert_eq!(imported.len(), 1);
    assert_eq!(owners(imported[0]), vec!["ApiError"]);

    // Type mentions are searchable.
    let list = find(&tags, "List", OccKind::Type);
    assert_eq!(list.len(), 2, "the alias and the extension receiver");
    assert!(
        find(&tags, "ApiError", OccKind::Type).len() >= 3,
        "return types and type arguments mention the class"
    );
}
