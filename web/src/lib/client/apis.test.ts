import { create } from '@bufbuild/protobuf';
import { describe, expect, it } from 'vitest';
import { DefinitionSchema } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { apiGroups } from './apis.ts';

const definitions = [
  { id: 'users#listUsers', name: 'List users', path: 'services/users/users.api.yaml' },
  { id: 'users#createUser', name: 'Create a user', path: 'services/users/users.api.yaml' },
  { id: 'orders#placeOrder', path: 'services/orders/orders.api.yaml' },
].map((d) => create(DefinitionSchema, d));

describe('apiGroups', () => {
  it('groups apis by file in order, naming an unnamed api by its ID', () => {
    expect(apiGroups(definitions)).toEqual([
      {
        key: 'users#',
        label: 'users',
        leaves: [
          { key: 'users#listUsers', label: 'List users', title: 'users#listUsers' },
          { key: 'users#createUser', label: 'Create a user', title: 'users#createUser' },
        ],
      },
      {
        key: 'orders#',
        label: 'orders',
        leaves: [{ key: 'orders#placeOrder', label: 'placeOrder', title: 'orders#placeOrder' }],
      },
    ]);
  });

  it('keeps only the apis whose ID or name matches the filter', () => {
    expect(apiGroups(definitions, ' USER').map((g) => g.leaves.length)).toEqual([2]);
  });
});
