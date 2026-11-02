## Summary

<!-- What this PR does and why, in two or three sentences. -->

## Changelog

-

## Review checklist

See `CONTRIBUTING.md` for what each item means.

- [ ] Names say what things are and do
- [ ] No speculative generality: every new trait or pattern has two implementations or a test that needs the seam
- [ ] No logic duplicated in a different shape
- [ ] Law of Demeter: no reaching through chains of other objects
- [ ] Tell, don't ask: behaviour sits with the data it uses
- [ ] No feature envy: no function that mostly works on another type's data
- [ ] New kinds are registry entries, not a string `match`
- [ ] Design docs updated in this PR where the change touches them
- [ ] Acceptance test written first
- [ ] Any change to a `decision-log.md` decision was asked, not decided in code

Gates: `mise run gates` passed
