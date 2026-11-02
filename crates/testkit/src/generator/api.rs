use super::rng::{Rng, fill, title};

/// File-level keys and the create, read and list apis every service has.
const CORE: &str = "name: %title% service
baseUrl: ${var.%svc%Url}/api/v1
headers:
  x-service: %svc%
vars:
  term: %word%
profiles:
  - id: eu
    vars:
      term: %word%-eu

apis:
  - id: create
    name: Create a record
    method: POST
    path: /%svc%
    vars:
      label: item-${run.id}
      quantity: %qty%
    body:
      label: ${var.label | trim}
      quantity: ${var.quantity | int}
      tags: [generated, %svc%]
    expect:
      status: 201
      headers:
        content-type:
          contains: json
    save:
      itemId: ${response.body.id}
      itemLabel: ${response.body.label | default('none')}
    cleanup:
      call: delete
      vars:
        itemId: ${response.body.id}

  - id: get
    name: Get a record
    path: /%svc%/${var.itemId}
    expect:
      status: 200
      body:
        match:
          id:
            type: string
      cel:
        has an id: has(response.body.id)

  - id: list
    name: List records
    path: /%svc%
    query:
      limit: ${var.pageSize | string}
    expect:
      status: 2xx
      body:
        match:
          items:
            type: array
      size:
        lt: 1mb
      timing:
        total:
          lt: 2s
";

/// Search, rename and delete, which every service has too.
const CHANGES: &str = "
  - id: search
    name: Search records
    path: /%svc%/search
    query:
      q: ${var.term | lower | urlEncode}
    expect:
      body:
        match:
          items[*].id:
            every:
              type: string

  - id: update
    name: Rename a record
    method: PATCH
    path: /%svc%/${var.itemId}
    vars:
      label: renamed
    body: {\"label\": \"${var.label}\", \"source\": \"generated\"}

  - id: delete
    name: Delete a record
    method: DELETE
    path: /%svc%/${var.itemId}
    expect:
      status: [200, 204, 404]
";

/// An extra state change; each service has a few of these.
const ACTION: &str = "
  - id: %verb%
    name: %title% a record
    method: POST
    path: /%svc%/${var.itemId}/%verb%
    headers:
      x-reason: %word%
    expect:
      status:
        in: [200, 202]
      body:
        match:
          state:
            startsWith: %verb%
";

/// The verbs extra apis are named after.
const VERBS: &[&str] = &["archive", "restore", "publish", "approve", "lock", "unlock", "share", "flag", "verify", "close"];

/// The api file of service `svc`.
pub fn render(svc: &str, rng: &mut Rng) -> String {
    let qty = rng.between(1, 9).to_string();
    let values = [("svc", svc), ("title", &title(svc)), ("word", rng.word()), ("qty", &qty)];
    let mut text = fill(CORE, &values);
    text.push_str(&fill(CHANGES, &values));
    let count = rng.between(2, 6);
    for index in rng.subset(VERBS.len(), count) {
        let verb = VERBS[index];
        text.push_str(&fill(ACTION, &[("svc", svc), ("verb", verb), ("title", &title(verb)), ("word", rng.word())]));
    }
    text
}
