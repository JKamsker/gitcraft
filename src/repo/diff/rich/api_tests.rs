//! Tests of API contract changes, on OpenAPI 3 and Swagger 2 documents.

use super::api::{self, ApiChange, Severity};
use super::*;

fn sides(old: &str, new: &str) -> Sides {
    Sides {
        old: Some(old.to_owned()),
        new: Some(new.to_owned()),
    }
}

fn changes(path: &str, old: &str, new: &str) -> Vec<ApiChange> {
    api::diff(path, &sides(old, new))
        .map(|d| d.changes)
        .unwrap_or_default()
}

fn said(c: &[ApiChange]) -> Vec<String> {
    c.iter()
        .map(|c| format!("{} {}", c.endpoint, c.what))
        .collect()
}

const BEFORE: &str = r#"openapi: 3.0.3
info: {title: Pets, version: "1"}
paths:
  /pets:
    get:
      parameters:
        - name: limit
          in: query
          required: false
          schema: {type: integer}
        - $ref: '#/components/parameters/Sort'
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema:
                type: array
                items: {$ref: '#/components/schemas/Pet'}
    post:
      requestBody:
        required: false
        content:
          application/json:
            schema: {$ref: '#/components/schemas/NewPet'}
      responses:
        "201": {description: created}
  /pets/{id}:
    delete:
      responses:
        "204": {description: gone}
components:
  parameters:
    Sort:
      name: sort
      in: query
      schema: {type: string, enum: [asc, desc]}
  schemas:
    Pet:
      type: object
      required: [id, name]
      properties:
        id: {type: integer}
        name: {type: string}
        tag: {type: string}
        kind: {type: string, enum: [cat, dog]}
        parent: {$ref: '#/components/schemas/Pet'}
    NewPet:
      type: object
      required: [name]
      properties:
        name: {type: string}
        age: {type: integer}
"#;

#[test]
fn breaking_changes_are_found_with_their_endpoint_and_lines() {
    let after = BEFORE
        // A parameter becomes required, a new required one appears.
        .replace("          required: false\n          schema: {type: integer}", "          required: true\n          schema: {type: integer}\n        - name: owner\n          in: header\n          required: true\n          schema: {type: string}")
        // A referenced parameter narrows what it accepts.
        .replace("enum: [asc, desc]", "enum: [asc]")
        // The response loses a field, and one is no longer always there.
        .replace("        tag: {type: string}\n", "")
        .replace("required: [id, name]", "required: [id]")
        // A response may now return a value clients don't know.
        .replace("enum: [cat, dog]", "enum: [cat, dog, bird]")
        // The request body becomes required, with a new required field.
        .replace("        required: false\n        content:", "        required: true\n        content:")
        .replace("      required: [name]\n", "      required: [name, age]\n")
        // An endpoint goes, another comes.
        .replace("  /pets/{id}:\n    delete:", "  /pets/{id}:\n    get:");
    let c = changes("openapi.yaml", BEFORE, &after);
    let mut got = said(&c);
    got.sort();
    let mut want = vec![
        "DELETE /pets/{id} Endpoint removed",
        "GET /pets The query parameter `limit` is now required",
        "GET /pets New required header parameter `owner`",
        "GET /pets Query parameter `sort` no longer accepts `desc`",
        "GET /pets Response `200` items field `tag` removed",
        "GET /pets Response `200` items field `name` is no longer always returned",
        "GET /pets Response `200` items field `kind` can now return `bird`",
        "POST /pets The request body is now required",
        "POST /pets Request body field `age` is now required",
        "GET /pets/{id} Endpoint added",
    ];
    want.sort();
    assert_eq!(got, want);
    // Breaking ones first.
    assert!(c.iter().take(9).all(|c| c.severity == Severity::Breaking));
    // Each opens its place: a removal where it was, the rest where they are now.
    let removed = c.iter().find(|c| c.what == "Endpoint removed");
    assert_eq!(
        removed.map(|c| (c.old_line, c.new_line)),
        Some((Some(29), None))
    );
    let owner = c.iter().find(|c| c.what.contains("owner"));
    assert_eq!(owner.and_then(|c| c.new_line), Some(11));
}

#[test]
fn nothing_breaks_when_only_things_are_added_or_described() {
    let after = BEFORE
        .replace("description: ok", "description: the pets")
        .replace(
            "        age: {type: integer}\n",
            "        age: {type: integer}\n        color: {type: string}\n",
        );
    let c = changes("api.yaml", BEFORE, &after);
    assert!(c.is_empty(), "{:?}", said(&c));
    let d = api::diff("api.yaml", &sides(BEFORE, BEFORE));
    assert_eq!(d.map(|d| d.version), Ok("OpenAPI 3.0.3".to_owned()));
}

#[test]
fn swagger_2_bodies_and_types() {
    let before = r#"{
  "swagger": "2.0",
  "paths": {
    "/users": {
      "post": {
        "parameters": [
          {"name": "body", "in": "body", "required": true,
           "schema": {"type": "object", "properties": {"email": {"type": "string"}}}}
        ],
        "responses": {"200": {"description": "ok", "schema": {"type": "object", "required": ["id"], "properties": {"id": {"type": "integer"}}}}}
      }
    }
  }
}"#;
    let after = before
        .replace(
            "\"properties\": {\"email\": {\"type\": \"string\"}}",
            "\"required\": [\"email\"], \"properties\": {\"email\": {\"type\": \"string\"}}",
        )
        .replace(
            "\"id\": {\"type\": \"integer\"}",
            "\"id\": {\"type\": \"string\"}",
        );
    let c = changes("swagger.json", before, &after);
    assert_eq!(
        said(&c),
        [
            "POST /users Request body field `email` is now required",
            "POST /users Response `200` field `id` changed type: integer → string",
        ]
    );
    // A success response removed.
    let gone = before.replace("\"200\"", "\"404\"");
    assert_eq!(
        said(&changes("swagger.json", before, &gone)),
        ["POST /users Response `200` removed"]
    );
}

#[test]
fn not_an_api_document_falls_back_with_a_note() {
    let d = api::diff("openapi.yaml", &sides("a: 1\n", "a: 2\n"));
    assert!(d.is_err_and(|e| e.contains("isn't an OpenAPI or Swagger document")));
    let broken = api::diff("openapi.yaml", &sides(BEFORE, "openapi: [\n"));
    assert!(broken.is_err_and(|e| e.contains("isn't valid")));
    let deleted = api::diff(
        "openapi.yaml",
        &Sides {
            old: Some(BEFORE.to_owned()),
            new: None,
        },
    );
    assert!(deleted.is_ok_and(|d| d.changes.len() == 1));
}

#[test]
fn api_kinds_are_offered_by_name_and_accepted_for_any_json_or_yaml() {
    assert_eq!(
        RichKind::of("api/openapi.yaml"),
        [RichKind::Config, RichKind::Api]
    );
    assert_eq!(
        RichKind::of("Swagger.JSON"),
        [RichKind::Config, RichKind::Api]
    );
    assert_eq!(RichKind::of("spec.yaml"), [RichKind::Config]);
    assert!(RichKind::Api.accepts("spec.yaml"));
    assert!(!RichKind::Api.accepts("spec.toml"));
}

/// A spec with thousands of endpoints and recursive schemas compares quickly.
#[test]
fn a_big_recursive_spec_compares_quickly() {
    let mut spec = String::from("openapi: 3.1.0\npaths:\n");
    for n in 0..3000 {
        spec.push_str(&format!("  /r{n}:\n    get:\n      responses:\n        \"200\":\n          content:\n            application/json:\n              schema: {{$ref: '#/components/schemas/Node'}}\n"));
    }
    spec.push_str("components:\n  schemas:\n    Node:\n      type: object\n      required: [child]\n      properties:\n        child: {$ref: '#/components/schemas/Node'}\n        v: {type: integer}\n");
    let after = spec.replace("required: [child]", "required: []");
    let started = std::time::Instant::now();
    let c = changes("openapi.yaml", &spec, &after);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(c.len(), 3000);
}

/// The view lists the changes and a click opens a change's place in the file.
#[test]
fn the_api_view_opens_the_place_of_a_change() -> Result<(), String> {
    use egui_kittest::kittest::Queryable;
    let after = BEFORE.replace("  /pets/{id}:\n    delete:", "  /pets/{id}:\n    get:");
    let d = api::diff("openapi.yaml", &sides(BEFORE, &after))?;
    let diff = RichDiff::Api(d);
    let acts = std::cell::RefCell::new(Vec::new());
    let state = std::cell::RefCell::new(RichState::default());
    let fonts = std::cell::Cell::new(false);
    let mut harness = egui_kittest::Harness::builder()
        .with_size([800.0, 400.0])
        .build_ui(|ui| {
            if !fonts.replace(true) {
                crate::theme::install_fonts(ui.ctx());
                return;
            }
            show(ui, &diff, &mut state.borrow_mut(), &mut acts.borrow_mut());
        });
    harness.run();
    harness
        .get_by_label_contains("DELETE /pets/{id} Endpoint removed")
        .click();
    harness.run();
    assert!(acts.borrow().iter().any(|a| matches!(
        a,
        Action::DiffGoToLine {
            old: Some(29),
            new: None
        }
    )));
    Ok(())
}

/// Read from a real repository through the same path the view takes.
#[test]
fn api_changes_are_read_from_a_repository() -> Result<(), String> {
    use crate::git;
    let dir = std::env::temp_dir().join(format!("gitcraft-rich-api-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(dir.join("spëc dir"));
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
    ] {
        git::run(Some(&dir), args)?;
    }
    let name = "spëc dir/pets api.yaml";
    let _ = std::fs::write(dir.join(name), BEFORE);
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "one"])?;
    let _ = std::fs::write(
        dir.join(name),
        BEFORE.replace("  /pets/{id}:\n    delete:", "  /pets/{id}:\n    get:"),
    );
    git::run(Some(&dir), &["commit", "-qam", "two"])?;
    let key = DiffKey::Commit {
        hash: git::head(&dir),
        path: name.to_owned(),
    };
    let read = load(&dir, &key, RichKind::Api, None);
    let _ = std::fs::remove_dir_all(&dir);
    let Ok(Rich::Ready(RichDiff::Api(d))) = read else {
        return Err("not read as an API".to_owned());
    };
    assert_eq!(
        said(&d.changes),
        [
            "DELETE /pets/{id} Endpoint removed",
            "GET /pets/{id} Endpoint added"
        ]
    );
    Ok(())
}
