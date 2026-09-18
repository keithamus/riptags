#![cfg(feature = "java")]

use riptags::{OccKind, SymKind, tag_source};

mod support;
use support::{find, owners};

const JAVA_SOURCE: &str = r#"
package com.example.api;

import java.util.List;
import java.util.*;
import static java.util.Arrays.asList;

public class ApiError extends RuntimeException implements Serializable, Failing {
    private static final int CODE = 400;
    private final String message;

    public ApiError(String message) {
        this.message = message;
    }

    public static ApiError badRequest(String message) {
        return new ApiError(message);
    }

    public String render() {
        return Formatter.format(this.message);
    }

    public <T> List<T> wrap(T item) {
        List<T> out = new ArrayList<>();
        out.add(item);
        this.render();
        return out;
    }

    public ApiError fail() {
        return this;
    }

    static class Inner {
        int depth;
    }
}

interface Failing extends Serializable {
    ApiError fail();

    default String tag() {
        return "fail";
    }
}

abstract class Base {
    abstract void run();
}

enum Level {
    LOW,
    HIGH
}

record Point(int x, int y) {}

@interface Marker {}
"#;

#[test]
fn java_types_own_their_members_and_references_resolve() {
    let tags = tag_source("com/example/api/ApiError.java", JAVA_SOURCE);

    // Type declarations map onto the right symbol kinds. The class shares its
    // name token with the constructor, so both definitions land on "ApiError".
    let api_error = find(&tags, "ApiError", OccKind::Definition);
    assert_eq!(api_error.len(), 2, "the class and its constructor");
    let class_def = api_error
        .iter()
        .find(|t| t.sym_kind == SymKind::Class)
        .expect("class definition");
    assert_eq!(class_def.line, 8);
    assert_eq!(owners(class_def), Vec::<&str>::new());

    let failing = find(&tags, "Failing", OccKind::Definition);
    assert_eq!(failing.len(), 1);
    assert_eq!(failing[0].sym_kind, SymKind::Interface);

    let level = find(&tags, "Level", OccKind::Definition);
    assert_eq!(level.len(), 1);
    assert_eq!(level[0].sym_kind, SymKind::Class, "enums are class-shaped");

    let point = find(&tags, "Point", OccKind::Definition);
    assert_eq!(point.len(), 1);
    assert_eq!(
        point[0].sym_kind,
        SymKind::Class,
        "records are class-shaped"
    );

    let marker = find(&tags, "Marker", OccKind::Definition);
    assert_eq!(marker.len(), 1);
    assert_eq!(marker[0].sym_kind, SymKind::Interface);

    // A method definition is owned by its type and carries it as context.
    let render = find(&tags, "render", OccKind::Definition);
    assert_eq!(render.len(), 1);
    assert_eq!(render[0].sym_kind, SymKind::Method);
    assert_eq!(owners(render[0]), vec!["ApiError"]);
    assert_eq!(render[0].context.as_deref(), Some("ApiError"));

    // A body-less interface member only declares; the class method defines.
    let fail_decl = find(&tags, "fail", OccKind::Declaration);
    assert_eq!(fail_decl.len(), 1);
    assert_eq!(owners(fail_decl[0]), vec!["Failing"]);
    let fail_def = find(&tags, "fail", OccKind::Definition);
    assert_eq!(fail_def.len(), 1);
    assert_eq!(owners(fail_def[0]), vec!["ApiError"]);

    // An abstract method is a declaration; a default method is a definition.
    let run = find(&tags, "run", OccKind::Declaration);
    assert_eq!(run.len(), 1);
    assert_eq!(owners(run[0]), vec!["Base"]);
    let tag_def = find(&tags, "tag", OccKind::Definition);
    assert_eq!(tag_def.len(), 1);
    assert_eq!(owners(tag_def[0]), vec!["Failing"]);

    // Constructors are methods of their class.
    let ctor_method = api_error
        .iter()
        .find(|t| t.sym_kind == SymKind::Method)
        .expect("constructor tagged as a method");
    assert_eq!(ctor_method.kind, OccKind::Definition);
    assert_eq!(owners(ctor_method), vec!["ApiError"]);

    // `static final` is a constant, a plain field stays a field, and enum
    // constants and record components are owned members too.
    let code = find(&tags, "CODE", OccKind::Definition);
    assert_eq!(code.len(), 1);
    assert_eq!(code[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(code[0]), vec!["ApiError"]);

    let message = find(&tags, "message", OccKind::Definition);
    assert_eq!(message.len(), 1);
    assert_eq!(message[0].sym_kind, SymKind::Field);
    assert_eq!(owners(message[0]), vec!["ApiError"]);

    let low = find(&tags, "LOW", OccKind::Definition);
    assert_eq!(low.len(), 1);
    assert_eq!(low[0].sym_kind, SymKind::Constant);
    assert_eq!(owners(low[0]), vec!["Level"]);

    let x = find(&tags, "x", OccKind::Definition);
    assert_eq!(x.len(), 1);
    assert_eq!(x[0].sym_kind, SymKind::Field);
    assert_eq!(owners(x[0]), vec!["Point"]);

    // Nested types nest their owners.
    let inner = find(&tags, "Inner", OccKind::Definition);
    assert_eq!(inner.len(), 1);
    assert_eq!(owners(inner[0]), vec!["ApiError"]);
    let depth = find(&tags, "depth", OccKind::Definition);
    assert_eq!(depth.len(), 1);
    assert_eq!(owners(depth[0]), vec!["Inner"]);

    // `Type.method()` resolves its owner through @qualifier ...
    let format = find(&tags, "format", OccKind::Call);
    assert_eq!(format.len(), 1);
    assert_eq!(owners(format[0]), vec!["Formatter"]);
    assert_eq!(format[0].context.as_deref(), Some("ApiError::render"));

    // ... `this.method()` through the enclosing type ...
    let this_render = find(&tags, "render", OccKind::Call);
    assert_eq!(this_render.len(), 1);
    assert_eq!(owners(this_render[0]), vec!["ApiError"]);

    // ... and a lowercase receiver claims no owner.
    let add = find(&tags, "add", OccKind::Call);
    assert_eq!(add.len(), 1);
    assert_eq!(owners(add[0]), Vec::<&str>::new());

    // `new Type(...)` is a call on the type.
    let new_api_error = find(&tags, "ApiError", OccKind::Call);
    assert_eq!(new_api_error.len(), 1);
    assert_eq!(
        new_api_error[0].context.as_deref(),
        Some("ApiError::badRequest")
    );
    assert_eq!(find(&tags, "ArrayList", OccKind::Call).len(), 1);

    // `extends` and `implements`, on classes and on interfaces, are impls.
    let runtime = find(&tags, "RuntimeException", OccKind::Impl);
    assert_eq!(runtime.len(), 1);
    assert_eq!(runtime[0].context.as_deref(), Some("ApiError"));
    let serializable = find(&tags, "Serializable", OccKind::Impl);
    assert_eq!(
        serializable.len(),
        2,
        "class implements and interface extends"
    );
    assert_eq!(find(&tags, "Failing", OccKind::Impl).len(), 1);

    // Imports: a plain one names the type, a static one resolves the owning
    // class from the package path, and a wildcard import names nothing.
    let list_import = find(&tags, "List", OccKind::Import);
    assert_eq!(list_import.len(), 1);
    assert_eq!(list_import[0].line, 4);
    let as_list = find(&tags, "asList", OccKind::Import);
    assert_eq!(as_list.len(), 1);
    assert_eq!(owners(as_list[0]), vec!["Arrays"]);
    assert!(
        find(&tags, "util", OccKind::Import).is_empty(),
        "`import java.util.*` names a package, not a symbol"
    );

    // Field access resolves its owner the same way calls do.
    let message_field = find(&tags, "message", OccKind::Field);
    assert_eq!(message_field.len(), 2, "both `this.message` reads");
    assert!(message_field.iter().all(|t| owners(t) == vec!["ApiError"]));

    // Type mentions in signatures are tagged; type parameters are not.
    let string_type = find(&tags, "String", OccKind::Type);
    assert!(
        string_type.len() >= 3,
        "field, constructor parameter and return types, got {}",
        string_type.len()
    );
    assert!(
        tags.iter().all(|t| &*t.name != "T"),
        "type parameters are signature-local, not searchable symbols"
    );

    // The package declaration is a module definition.
    let pkg = find(&tags, "api", OccKind::Definition);
    assert_eq!(pkg.len(), 1);
    assert_eq!(pkg[0].sym_kind, SymKind::Module);
}
