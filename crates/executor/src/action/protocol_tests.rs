use vc_model::Protocol;

use super::ProtocolRuntimes;

#[test]
fn the_standard_runtimes_cover_every_protocol() {
    let runtimes = ProtocolRuntimes::standard();
    assert_eq!(runtimes.protocols(), Protocol::ALL);
    for protocol in Protocol::ALL {
        assert_eq!(runtimes.get(protocol).map(|runtime| runtime.protocol()), Some(protocol));
    }
}
