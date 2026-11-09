//! Computed bodies: `$extend` merges changes into an earlier response, `$remove` drops fields,
//! and `$cel` computes a value, all as the server receives them.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, SCRIPTED};

#[tokio::test(flavor = "multi_thread")]
async fn an_extended_body_keeps_the_original_with_the_changes_and_cel_computes_values() {
    let server = MockServer::start().await.unwrap();
    let test = concat!(
        "tests:\n  - id: resendsChangedOrder\n    vars:\n      price: 21\n      total: 150.5\n    steps:\n",
        "      - id: original\n        call: scripted#echo\n",
        "        body: {id: 7, items: [a, b], shipping: {city: Oslo, zip: '0150', geo: {lat: 1}}}\n",
        "      - call: scripted#echo\n        body:\n          $extend: ${original.body.body}\n",
        "          shipping:\n            city: Bergen\n          doubled:\n            $cel: var.price * 2\n",
        "          tier:\n            $cel: \"var.total > 100.0 ? 'gold' : 'standard'\"\n",
        "          $remove: [id, shipping.geo]\n",
        "        expect:\n          body:\n            match:\n",
        "              body.items: [a, b]\n              body.shipping: {city: Bergen, zip: '0150'}\n",
        "              body.doubled: 42\n              body.tier: gold\n",
        "          cel:\n            dropped: \"!has(response.body.body.id) && !has(response.body.body.shipping.geo)\"\n",
    );
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/values.test.yaml", test)]);
    let output = project.run(&server.base_url(), &[]).assert().get_output().clone();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 2);
}
