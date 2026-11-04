/// The flow every other test file of a service reuses through `test:`; only the crud file has it.
pub const PREPARE: &str = "
  - id: preparesItem
    name: Prepares a record for the other files
    standalone: false
    steps:
      - call: %svc%#create
      - id: checkStep
        call: %svc%#get
";

/// The tests a file picks from. Step IDs differ between cases, so any selection keeps IDs unique.
pub const CASES: &[&str] = &[
    "
  - id: createsItem
    name: Creates a %svc% record
    call: %svc%#create
    vars:
      label: %word%-${run.id}
    expect:
      body:
        match:
          label:
            startsWith: %word%
          quantity:
            type: integer
",
    "
  - id: getsCreatedItem
    name: Returns the record that was just created
    call: %svc%#get
    expect:
      body:
        match:
          id: ${var.itemId}
",
    "
  - id: listsItems
    name: Lists one page of records
    call: %svc%#list
    vars:
      pageSize: %page%
    expect:
      body:
        match:
          items:
            length:
              lte: %page%
      cel:
        fits one page: size(response.body.items) <= %page%
",
    "
  - id: renamesItem
    name: A renamed record reads back with the new label
    steps:
      - id: renameStep
        call: %svc%#update
        vars:
          label: %word%-renamed
      - id: readStep
        call: %svc%#get
        expect:
          cel:
            keeps the new label: response.body.label == renameStep.body.label
    save:
      finalLabel: ${response.body.label}
",
    "
  - id: rejectsMissingItem
    name: Returns 404 for a record that does not exist
    call: %svc%#get
    vars:
      itemId: missing-%word%
    expect:
      replace: true
      status: 404
",
    "
  - id: readsSeededItem
    name: Reads the record the file seeded
    call: %svc%#get
    vars:
      itemId: ${var.seed.itemId}
    expect:
      body:
        match:
          label:
            startsWith: seed-
",
    "
  - id: readsAdminItem
    name: Reads the record the run made for the admin
    call: %first%#get
    vars:
      itemId: ${var.admin.itemId}
    expect:
      body:
        match:
          label:
            empty: false
",
    "
  - id: createsSeveralItems
    name: Creates a batch of records and lists them
    steps:
      - id: batchStep
        call: %svc%#create
        repeat: %count%
        parallel: 2
        vars:
          label: batch-${iteration.index}
        save:
          batchIds: ${response.body.id}
      - id: otherStep
        call: %other%#list
        expect:
          cel:
            made the whole batch: size(var.batchIds) == %count%
",
    "
  - id: rejectsBadQuantity
    name: Rejects quantities below one
    examples:
      - quantity: 0
      - quantity: -%qty%
    call: %svc%#create
    vars:
      quantity: ${example.quantity}
    expect:
      status: 400
      cel:
        names the field: response.body.errors.exists(e, e.field == 'quantity')
",
    "
  - id: tagsItemEachWay
    name: Applies every colour label in turn
    steps:
      - id: tagStep
        call: %svc%#update
        forEach: [red, green, blue]
        vars:
          label: ${item}
        expect:
          status:
            anyOf: [{eq: 200}, {eq: 204}]
",
    "
  - id: waitsForItemToSettle
    name: A new record settles within ten seconds
    call: %svc%#get
    poll:
      until:
        body:
          match:
            state:
              in: [ready, active]
      every: 1s
      timeout: 10s
",
    "
  - id: exportsItem
    name: Exports a record as a file
    skip: \"export endpoint not built yet\"
    call: %svc%#get
",
    "
  - id: doublesQuantity
    name: Stores a computed quantity
    vars:
      baseQuantity: %qty%
      quantity:
        $cel: var.baseQuantity * 2
    call: %svc%#create
    expect:
      body:
        match:
          quantity: %double%
          label:
            regex: \"^%area%-\"
",
];
