import { describe, expect, it } from 'vitest';
import { apiGroups } from './apis.ts';

const definitions = [
  { id: 'users#listUsers', name: 'List users', path: 'services/users/users.api.yaml', vars: [] },
  {
    id: 'users#createUser',
    name: 'Create a user',
    path: 'services/users/users.api.yaml',
    vars: ['email'],
  },
  {
    id: 'orders#placeOrder',
    name: 'placeOrder',
    path: 'services/orders/orders.api.yaml',
    vars: [],
  },
];

describe('apiGroups', () => {
  it('groups apis by file, in order', () => {
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
