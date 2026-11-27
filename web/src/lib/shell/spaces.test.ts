import { describe, expect, it } from 'vitest';
import { isKnown, isUnder, navOf, spaceByKey, spaceOf, spaces, viewOf } from './spaces.ts';

describe('spaces', () => {
  it('has the five spaces in rail order, Settings last', () => {
    expect(spaces.map((s) => s.key)).toEqual(['client', 'tests', 'runs', 'agents', 'settings']);
    expect(spaces.map((s) => s.shortcut)).toEqual(['1', '2', '3', '4', ',']);
  });

  it('puts the editor under Tests and the overview under Runs', () => {
    expect(spaceOf('/tests/editor').key).toBe('tests');
    expect(viewOf('/tests/editor')?.label).toBe('Editor');
    expect(spaceOf('/runs/overview').key).toBe('runs');
    expect(viewOf('/runs/overview')?.title).toBe('Runs overview');
    expect(viewOf('/runs')?.title).toBe('All runs');
  });

  it('matches the longest route a path is under', () => {
    expect(viewOf('/tests/editor/x')?.route).toBe('/tests/editor');
    expect(viewOf('/tests')?.route).toBe('/tests');
  });

  it('lights only the sidebar item a path is, never a parent view', () => {
    expect(navOf('/runs')?.label).toBe('All runs');
    expect(navOf('/runs/')?.label).toBe('All runs');
    expect(navOf('/runs/overview')?.label).toBe('Overview');
    expect(navOf('/runs/live')).toBeUndefined();
    expect(navOf('/runs/load')).toBeUndefined();
    expect(navOf('/runs/42')).toBeUndefined();
    expect(navOf('/tests/editor')?.label).toBe('Editor');
  });

  it('counts an unknown path as Client, and does not know it', () => {
    expect(spaceOf('/nowhere').key).toBe('client');
    expect(viewOf('/nowhere')).toBeUndefined();
    expect(isKnown('/nowhere')).toBe(false);
    expect(isKnown('/agents')).toBe(true);
  });

  it('tells a page under a route from a lookalike', () => {
    expect(isUnder('/runs/overview', '/runs')).toBe(true);
    expect(isUnder('/runsx', '/runs')).toBe(false);
  });

  it('finds a space by key', () => {
    expect(spaceByKey('agents').home).toBe('/agents');
  });
});
