/* Vibranium UI v2 mockups: fake data for the "shop" demo project.
   Everything the screens show comes from here, so pass-2 screens can add
   their own keys (VC.data.tests, VC.data.runs …) without touching app.js.
   Secrets are never stored as values: they carry only a source. */
window.VC = window.VC || {};
VC.data = {
  project: { name: 'shop', root: '~/dev/shop', branch: 's/checkout-v2', engine: 'vc 2.0.0-beta.3', tint: 'graphite', lastCheck: 'vc check · 0 errors · 2 warnings · 14:02' },

  profiles: [
    { id: 'local', baseUrl: 'http://localhost:7421', tint: null, note: 'vc demo mock' },
    { id: 'dev', baseUrl: 'https://dev.shop.internal', tint: null, note: 'shared dev' },
    { id: 'staging', baseUrl: 'https://staging.shop.example', tint: 'amber', note: 'pre-release' },
    { id: 'prod', baseUrl: 'https://api.shop.example', tint: 'red', note: 'live data' }
  ],
  layers: [
    { id: 'eu', note: 'region: eu-west-1' },
    { id: 'slowNetwork', note: 'timeout 30s, 2 retries' },
    { id: 'debug', note: 'history.bodies: all' }
  ],
  /* config explain: the merged value of every key and where it was set */
  explain: [
    { key: 'baseUrl', value: '${profile.baseUrl}', src: 'profile', at: 'vibranium.config.yaml:18 · profiles.<p>.baseUrl' },
    { key: 'timeout', value: '10s', src: 'config', at: 'vibranium.config.yaml:7 · defaults.timeout' },
    { key: 'retries', value: '1', src: 'config', at: 'vibranium.config.yaml:8 · defaults.retries' },
    { key: 'auth', value: 'shopAuth', src: 'profile', at: 'vibranium.config.yaml:21 · profiles.<p>.auth' },
    { key: 'headers.x-client', value: 'vibranium', src: 'config', at: 'vibranium.config.yaml:11 · http.headers' },
    { key: 'vars.tenantId', value: 't_shop_01', src: 'profile', at: 'vibranium.config.yaml:24 · profiles.<p>.vars' },
    { key: 'secret.API_TOKEN', value: '••••', src: 'secret', at: 'macOS keychain · vibranium/shop' }
  ],

  services: [
    { id: 'users', type: 'http', file: 'services/users/users.api.yaml', apis: ['users#list', 'users#get', 'users#create', 'users#update', 'users#delete'] },
    { id: 'orders', type: 'http', file: 'services/orders/orders.api.yaml', apis: ['orders#list', 'orders#create', 'orders#get'] },
    { id: 'payments', type: 'http', file: 'services/payments/payments.api.yaml', apis: ['payments#charge', 'payments#refund', 'payments#events'] },
    { id: 'catalog', type: 'graphql', file: 'services/catalog/catalog.api.yaml', apis: ['catalog#product', 'catalog#products'] },
    { id: 'rooms', type: 'websocket', file: 'services/rooms/rooms.api.yaml', apis: ['rooms#join'] }
  ],
  apis: {
    'users#list':   { name: 'List users', m: 'GET', path: '/users', q: '?limit=20', line: 12 },
    'users#get':    { name: 'Get user', m: 'GET', path: '/users/${userId}', q: '?expand=orders', line: 24 },
    'users#create': { name: 'Create user', m: 'POST', path: '/users', line: 38 },
    'users#update': { name: 'Update user', m: 'PATCH', path: '/users/${userId}', line: 57 },
    'users#delete': { name: 'Delete user', m: 'DELETE', path: '/users/${userId}', line: 71 },
    'orders#list':  { name: 'List orders', m: 'GET', path: '/orders', q: '?status=open', line: 9 },
    'orders#create':{ name: 'Create order', m: 'POST', path: '/orders', line: 21 },
    'orders#get':   { name: 'Get order', m: 'GET', path: '/orders/${orderId}', line: 44 },
    'payments#charge': { name: 'Charge card', m: 'POST', path: '/payments/charges', line: 14 },
    'payments#refund': { name: 'Refund charge', m: 'POST', path: '/payments/charges/${chargeId}/refunds', line: 33 },
    'payments#events': { name: 'Payment events', m: 'SSE', path: '/payments/events', line: 52, route: 'client-sse' },
    'catalog#product': { name: 'Product by id', m: 'GQL', path: '/api/graphql', line: 10, route: 'client-graphql' },
    'catalog#products':{ name: 'Products page', m: 'GQL', path: '/api/graphql', line: 31, route: 'client-graphql' },
    'rooms#join':   { name: 'Join lobby', m: 'WS', path: '/rooms/lobby', line: 8, route: 'client-ws' }
  },

  /* ${var} hovers: value and origin */
  vars: {
    baseUrl: { value: 'http://localhost:7421', src: 'profile', at: 'profiles.local.baseUrl · vibranium.config.yaml:18' },
    userId:  { value: 'u_8f2c41', src: 'set', at: 'saved by users.crud#createsUser · 2 min ago' },
    orderId: { value: 'o_51d9', src: 'set', at: 'saved by orders.checkout#placesOrder · 14:01' },
    chargeId:{ value: 'ch_3PqZ', src: 'set', at: 'saved by payments#charge · 13:58' },
    tenantId:{ value: 't_shop_01', src: 'profile', at: 'profiles.local.vars.tenantId' },
    'secret.API_TOKEN': { value: '••••', secret: true, src: 'secret', at: 'macOS keychain · vibranium/shop' }
  },

  pins: [
    { api: 'users#list', label: 'Users' },
    { api: 'orders#create', label: 'New order' },
    { api: 'catalog#product', label: 'Catalog' },
    { api: 'rooms#join', label: 'Lobby' },
    { api: 'payments#events', label: 'Pay events' },
    { route: 'client-auth', label: 'Auth', ico: 'key' }
  ],
  tabs: [
    { id: 't1', api: 'users#get' },
    { id: 't2', api: 'users#create' },
    { id: 't3', api: 'catalog#product' },
    { id: 't4', api: 'rooms#join' }
  ],

  history: [
    { day: 'Today', items: [
      { t: '14:06', m: 'GET', path: '/users/u_8f2c41', s: 200, ms: 142, src: 'ui', api: 'users#get' },
      { t: '14:04', m: 'POST', path: '/users', s: 201, ms: 188, src: 'ui', api: 'users#create' },
      { t: '13:58', m: 'POST', path: '/payments/charges', s: 402, ms: 311, src: 'cli', api: 'payments#charge' },
      { t: '13:41', m: 'GQL', path: 'query ProductById', s: 200, ms: 96, src: 'tui', api: 'catalog#product' },
      { t: '13:12', m: 'WS', path: '/rooms/lobby', s: 101, ms: 0, src: 'ui', api: 'rooms#join', note: '42 msgs' }
    ] },
    { day: 'Yesterday', items: [
      { t: '18:22', m: 'GET', path: '/orders?status=open', s: 200, ms: 77, src: 'cli', api: 'orders#list' },
      { t: '17:50', m: 'DELETE', path: '/users/u_77a0', s: 204, ms: 64, src: 'ui', api: 'users#delete' },
      { t: '16:03', m: 'SSE', path: '/payments/events', s: 200, ms: 0, src: 'tui', api: 'payments#events', note: '9 events' }
    ] },
    { day: 'Fri 20 Nov', items: [
      { t: '11:15', m: 'PATCH', path: '/users/u_8f2c41', s: 422, ms: 120, src: 'cli', api: 'users#update' },
      { t: '10:02', m: 'GET', path: '/users?limit=20', s: 200, ms: 58, src: 'ui', api: 'users#list' }
    ] }
  ],

  /* request tabs for users#get: inherited (read-only, with source) and this request */
  request: {
    params: {
      inherited: [
        { k: 'expand', v: 'orders', src: 'api', at: 'users.api.yaml:27' },
        { k: 'tenant', v: '${tenantId}', src: 'profile', at: 'profiles.local.vars' }
      ],
      own: [{ k: 'fields', v: 'id,name,email,role,createdAt' }]
    },
    headers: {
      inherited: [
        { k: 'accept', v: 'application/json', src: 'file', at: 'users.api.yaml:5' },
        { k: 'x-client', v: 'vibranium', src: 'config', at: 'vibranium.config.yaml:11' },
        { k: 'x-request-id', v: '${uuid()}', src: 'config', at: 'vibranium.config.yaml:12' },
        { k: 'authorization', v: 'Bearer ••••', src: 'secret', at: 'auth shopAuth' }
      ],
      own: [{ k: 'x-debug', v: 'true' }]
    },
    vars: {
      inherited: [
        { k: 'baseUrl', v: 'http://localhost:7421', src: 'profile', at: 'profiles.local' },
        { k: 'tenantId', v: 't_shop_01', src: 'profile', at: 'profiles.local.vars' },
        { k: 'userId', v: 'u_8f2c41', src: 'set', at: 'users.crud#createsUser' }
      ],
      own: []
    },
    options: {
      inherited: [
        { k: 'timeout', v: '10s', src: 'config', at: 'defaults.timeout' },
        { k: 'retries', v: '1', src: 'config', at: 'defaults.retries' },
        { k: 'followRedirects', v: 'true', src: 'config', at: 'http.followRedirects' },
        { k: 'http.version', v: 'auto (h2)', src: 'config', at: 'http.version' }
      ],
      own: [{ k: 'retries', v: '2' }]
    },
    expect: {
      inherited: [
        { k: 'status', v: '200', src: 'api', at: 'users.api.yaml:31' },
        { k: 'headers.content-type', v: '{ contains: json }', src: 'file', at: 'users.api.yaml:6' }
      ],
      own: [
        { k: 'body.id', v: '${userId}' },
        { k: 'body.email', v: '{ regex: "^[^@]+@shop\\.test$" }' }
      ]
    }
  },

  response: {
    status: 200, text: 'OK', ms: 142, size: '1.84 KB', proto: 'h2', retried: true, authRefreshed: true,
    body: {
      id: 'u_8f2c41', name: 'Ada Lovelace', email: 'ada@shop.example', role: 'admin', active: true,
      createdAt: '2026-11-23T13:04:51Z', lastLogin: null,
      address: { city: 'London', postcode: 'NW1 6XE', country: 'GB' },
      tags: ['beta', 'early-access'],
      orders: [
        { id: 'o_51d9', total: 129.5, currency: 'GBP', status: 'open', items: 3 },
        { id: 'o_4410', total: 42, currency: 'GBP', status: 'shipped', items: 1 }
      ],
      limits: { requestsPerMinute: 600, burst: 50 }
    },
    headers: [
      ['content-type', 'application/json; charset=utf-8'], ['content-length', '1884'], ['cache-control', 'private, max-age=0'],
      ['etag', 'W/"75c-1f3a"'], ['x-request-id', '0b7e1c22-5c43-4c1e-9f8a-2f1a6e4b9d10'], ['x-ratelimit-remaining', '597'],
      ['set-cookie', 'session=••••; Path=/; HttpOnly; Secure; SameSite=Strict'], ['set-cookie', 'region=eu; Path=/; Max-Age=86400'],
      ['date', 'Mon, 23 Nov 2026 14:06:12 GMT'], ['server', 'vc-demo/2.0']
    ],
    cookies: [
      { name: 'session', value: '••••', domain: 'localhost', path: '/', expires: 'Session', flags: 'HttpOnly · Secure · SameSite=Strict' },
      { name: 'region', value: 'eu', domain: 'localhost', path: '/', expires: 'in 24 h', flags: '' }
    ],
    timing: [
      { k: 'DNS', cls: 'dns', start: 0, d: 4 },
      { k: 'Connect', cls: 'connect', start: 4, d: 9 },
      { k: 'TLS', cls: 'tls', start: 13, d: 18 },
      { k: 'Request sent', cls: 'send', start: 31, d: 2 },
      { k: 'Waiting (TTFB)', cls: 'wait', start: 33, d: 96 },
      { k: 'Download', cls: 'down', start: 129, d: 13 }
    ],
    events: [
      { t: '−420 ms', kind: 'auth', ico: 'key', title: 'Auth refresh: shopAuth', sub: 'oauth2ClientCredentials token expired 12 s ago. POST /oauth/token → 200 in 38 ms. Token cached for 59 min.' },
      { t: '−210 ms', kind: 'retry', ico: 'refresh', title: 'Retry 1 of 2 after 503 Service Unavailable', sub: 'Backoff 200 ms (defaults.retries, retryOn: [502, 503, 504]).' },
      { t: '0 ms', kind: 'redirect', ico: 'redirect', title: 'No redirects', sub: 'followRedirects: true' },
      { t: '0 ms', kind: 'sign', ico: 'signature', title: 'Signing: none', sub: 'shopAuth adds a bearer header; no awsSigV4 on this api.' }
    ],
    checks: [
      { st: 'pass', what: 'status', exp: '200', got: '200', from: 'api' },
      { st: 'pass', what: 'headers.content-type', exp: '{ contains: json }', got: 'application/json; charset=utf-8', from: 'file' },
      { st: 'pass', what: 'body.id', exp: '${userId}', got: '"u_8f2c41"', from: 'request' },
      { st: 'fail', what: 'body.email', exp: '{ regex: "^[^@]+@shop\\.test$" }', got: '"ada@shop.example"', from: 'request', code: 'V5120' }
    ],
    saved: [
      { k: 'userEmail', v: 'ada@shop.example', from: 'save: { userEmail: body.email }' },
      { k: 'firstOrderId', v: 'o_51d9', from: 'save: { firstOrderId: body.orders[0].id }' }
    ]
  },

  /* GraphQL: catalog#product */
  gql: {
    query: 'query ProductById($id: ID!, $withCost: Boolean = true) {\n  product(id: $id) {\n    id\n    title\n    price { amount currency }\n    stock\n    costPrice @include(if: $withCost)\n    reviews(first: 2) {\n      rating\n      author { name }\n    }\n  }\n}',
    variables: '{\n  "id": "p_1042",\n  "withCost": true\n}',
    data: {
      data: {
        product: { id: 'p_1042', title: 'Merino crew, slate', price: { amount: 89, currency: 'GBP' }, stock: 14, costPrice: null,
          reviews: [{ rating: 5, author: { name: 'Grace H.' } }, { rating: 4, author: { name: 'Alan T.' } }] }
      },
      errors: [{ message: 'Not authorised to read Product.costPrice', path: ['product', 'costPrice'], extensions: { code: 'FORBIDDEN', requires: 'catalog:cost' } }]
    },
    schema: [
      { name: 'Query', kind: 'type', fields: [
        { n: 'product', a: '(id: ID!)', t: 'Product', d: 'One product by id.' },
        { n: 'products', a: '(first: Int = 20, after: String, filter: ProductFilter)', t: 'ProductConnection!', d: 'A page of products.' },
        { n: 'categories', a: '', t: '[Category!]!', d: 'Every category, flat.' }
      ] },
      { name: 'Product', kind: 'type', fields: [
        { n: 'id', a: '', t: 'ID!' }, { n: 'title', a: '', t: 'String!' }, { n: 'price', a: '', t: 'Money!' },
        { n: 'stock', a: '', t: 'Int!', d: 'Units on hand across warehouses.' },
        { n: 'costPrice', a: '', t: 'Money', dir: '@auth(requires: "catalog:cost")', d: 'Needs the catalog:cost scope; null with a FORBIDDEN error otherwise.' },
        { n: 'reviews', a: '(first: Int = 10)', t: '[Review!]!' }
      ] },
      { name: 'Money', kind: 'type', fields: [{ n: 'amount', a: '', t: 'Float!' }, { n: 'currency', a: '', t: 'Currency!' }] },
      { name: 'Review', kind: 'type', fields: [{ n: 'rating', a: '', t: 'Int!' }, { n: 'author', a: '', t: 'User!' }, { n: 'body', a: '', t: 'String' }] },
      { name: 'ProductFilter', kind: 'input', fields: [{ n: 'category', a: '', t: 'ID' }, { n: 'inStock', a: '', t: 'Boolean' }] },
      { name: 'Currency', kind: 'enum', fields: [{ n: 'GBP', a: '', t: '' }, { n: 'EUR', a: '', t: '' }, { n: 'USD', a: '', t: '' }] }
    ]
  },

  /* WebSocket rooms#join transcript, and the simulated feed */
  ws: {
    url: 'ws://localhost:7421/rooms/lobby',
    transcript: [
      { t: '14:06:01.112', dir: 'sys', kind: 'open', size: 0, data: '101 Switching Protocols · h1 upgrade' },
      { t: '14:06:01.140', dir: 'out', kind: 'text', size: 38, data: { type: 'join', name: 'ada', room: 'lobby' } },
      { t: '14:06:01.162', dir: 'in', kind: 'text', size: 61, data: { kind: 'welcome', room: 'lobby', members: 3 } },
      { t: '14:06:01.170', dir: 'in', kind: 'text', size: 214, data: { kind: 'history', items: [{ id: 1, from: 'grace', text: 'morning' }, { id: 2, from: 'alan', text: 'deploy at 3?' }] } },
      { t: '14:06:04.508', dir: 'in', kind: 'text', size: 44, data: { kind: 'presence', who: 'linus', state: 'joined' } },
      { t: '14:06:09.930', dir: 'out', kind: 'text', size: 41, data: { type: 'say', text: 'hello lobby' } },
      { t: '14:06:09.951', dir: 'in', kind: 'text', size: 66, data: { kind: 'message', id: 3, from: 'ada', text: 'hello lobby' } }
    ],
    feed: [
      { kind: 'typing', who: 'grace' },
      { kind: 'message', id: 4, from: 'grace', text: 'hi ada' },
      { kind: 'presence', who: 'alan', state: 'away' },
      { kind: 'ping', at: 1764079600 },
      { kind: 'message', id: 5, from: 'linus', text: 'ship it' },
      { kind: 'typing', who: 'alan' }
    ],
    snippets: [
      { name: 'Join lobby', body: '{ "type": "join", "name": "ada", "room": "lobby" }' },
      { name: 'Say hello', body: '{ "type": "say", "text": "hello lobby" }' },
      { name: 'Ping', body: '{ "type": "ping" }' },
      { name: 'Leave', body: '{ "type": "leave" }' }
    ]
  },
  sse: {
    url: '${baseUrl}/payments/events',
    transcript: [
      { t: '13:58:10.004', dir: 'sys', kind: 'open', size: 0, data: '200 OK · text/event-stream' },
      { t: '13:58:10.006', dir: 'in', kind: 'retry', size: 11, data: 'retry: 3000' },
      { t: '13:58:12.430', dir: 'in', kind: 'charge.created', id: '101', size: 96, data: { charge: 'ch_3PqZ', amount: 12950, currency: 'gbp' } },
      { t: '13:58:12.902', dir: 'in', kind: 'charge.failed', id: '102', size: 118, data: { charge: 'ch_3PqZ', code: 'card_declined', decline: 'insufficient_funds' } },
      { t: '13:58:20.000', dir: 'in', kind: 'heartbeat', id: '103', size: 18, data: 'ok' }
    ],
    feed: [
      { kind: 'charge.created', data: { charge: 'ch_3Pr1', amount: 4200, currency: 'gbp' } },
      { kind: 'charge.succeeded', data: { charge: 'ch_3Pr1' } },
      { kind: 'heartbeat', data: 'ok' },
      { kind: 'refund.created', data: { refund: 're_9a', charge: 'ch_3Pr1', amount: 4200 } }
    ]
  },

  /* auth registry */
  auth: [
    { id: 'basic', label: 'Basic', ico: 'user', keys: ['username', 'password'] },
    { id: 'bearer', label: 'Bearer token', ico: 'key', keys: ['token'] },
    { id: 'apiKey', label: 'API key', ico: 'key', keys: ['in', 'name', 'value'] },
    { id: 'oauth2ClientCredentials', label: 'OAuth2 · client credentials', ico: 'shield', keys: ['tokenUrl', 'clientId', 'clientSecret', 'scopes', 'audience'], used: 'shopAuth' },
    { id: 'oauth2Password', label: 'OAuth2 · password', ico: 'shield', keys: ['tokenUrl', 'clientId', 'clientSecret', 'username', 'password', 'scopes'] },
    { id: 'oauth2AuthorizationCode', label: 'OAuth2 · auth code + PKCE', ico: 'shield', keys: ['authorizeUrl', 'tokenUrl', 'clientId', 'scopes', 'pkce', 'redirectPort'] },
    { id: 'oauth2Refresh', label: 'OAuth2 · refresh token', ico: 'shield', keys: ['tokenUrl', 'clientId', 'refreshToken', 'scopes'] },
    { id: 'flow', label: 'Flow (login steps)', ico: 'flow', keys: ['steps', 'apply', 'expiresIn', 'refreshBefore', 'cache'] },
    { id: 'awsSigV4', label: 'AWS SigV4', ico: 'aws', keys: ['region', 'service', 'accessKeyId', 'secretAccessKey', 'sessionToken'] },
    { id: 'mtls', label: 'mTLS client certificate', ico: 'cert', keys: ['clientCert'] },
    { id: 'jwt', label: 'JWT (signed locally)', ico: 'signature', coming: true },
    { id: 'digest', label: 'Digest', ico: 'lock', coming: true },
    { id: 'deviceCode', label: 'OAuth2 · device code', ico: 'shield', coming: true }
  ],
  authConfig: {
    oauth2ClientCredentials: [
      { k: 'tokenUrl', v: '${baseUrl}/oauth/token' }, { k: 'clientId', v: 'shop-tests' },
      { k: 'clientSecret', secret: 'OAUTH_CLIENT_SECRET', src: 'keychain' }, { k: 'scopes', v: 'users:read users:write catalog:read' },
      { k: 'audience', v: 'https://api.shop.example' }, { k: 'clientCert', v: 'none' }
    ],
    awsSigV4: [
      { k: 'region', v: 'eu-west-1' }, { k: 'service', v: 'execute-api' }, { k: 'accessKeyId', secret: 'AWS_ACCESS_KEY_ID', src: 'env' },
      { k: 'secretAccessKey', secret: 'AWS_SECRET_ACCESS_KEY', src: 'env' }, { k: 'sessionToken', secret: 'AWS_SESSION_TOKEN', src: 'env' }, { k: 'payload', v: 'signed' }
    ]
  },
  claims: [
    ['iss', 'http://localhost:7421'], ['sub', 'shop-tests'], ['aud', 'https://api.shop.example'], ['scope', 'users:read users:write catalog:read'],
    ['iat', '14:05:34 (32 s ago)'], ['exp', '15:05:34 (in 59 min)'], ['jti', '••••']
  ],

  /* import wizard */
  importOps: [
    { tag: 'users', ops: [['GET', '/users', 'listUsers'], ['POST', '/users', 'createUser'], ['GET', '/users/{id}', 'getUser'], ['PATCH', '/users/{id}', 'updateUser'], ['DELETE', '/users/{id}', 'deleteUser']] },
    { tag: 'orders', ops: [['GET', '/orders', 'listOrders'], ['POST', '/orders', 'createOrder'], ['GET', '/orders/{id}', 'getOrder']] },
    { tag: 'admin', ops: [['POST', '/admin/reindex', 'reindex'], ['DELETE', '/admin/cache', 'purgeCache']] }
  ],
  importSecrets: [
    { where: 'components.securitySchemes.bearerAuth', found: 'Bearer eyJhbGciOi…', name: 'API_TOKEN', src: 'keychain' },
    { where: 'servers[1].variables.apiKey.default', found: 'sk_test_51…', name: 'PAYMENTS_KEY', src: 'env' }
  ],

  /* command palette */
  commands: [
    { cmd: 'vc check', args: [], desc: 'Parse and check every file. Reports V1–V4 diagnostics with fixes.', preview: '42 files · profile local' },
    { cmd: 'vc run', args: [{ n: 'selector', t: 'selector', v: 'users.crud' }], desc: 'Run tests matching a selector.', preview: '6 tests in users.crud · profile local · concurrency 8' },
    { cmd: 'vc plan', args: [{ n: 'selector', t: 'selector', v: 'orders' }], desc: 'Show what a run would do, without sending anything.', preview: '3 files · 11 tests · 2 lifecycle blocks' },
    { cmd: 'vc fmt', args: [], desc: 'Format every YAML file in place (diff first).', preview: '2 files would change' },
    { cmd: 'vc rename', args: [{ n: 'from', t: 'id', v: 'users#get' }, { n: 'to', t: 'id', v: 'users#fetch' }], desc: 'Rename an id and every reference to it.', preview: '1 api · 4 references in 3 files' },
    { cmd: 'vc import', args: [{ n: 'kind', t: 'openapi|har|postman|bruno|http|curl', v: 'openapi' }, { n: 'source', t: 'path or URL', v: './openapi.yaml' }], desc: 'Import apis, then review a diff of every file.', preview: 'Opens the import wizard' },
    { cmd: 'vc sync', args: [], desc: 'Compare apis with their source.openapi specs.', preview: 'users drifted: 1 new operation' },
    { cmd: 'vc explain', args: [{ n: 'code', t: 'diagnostic code', v: 'V5128' }], desc: 'Explain a diagnostic code with an example and fix.', preview: 'V5128 · GraphQL response has data and errors' },
    { cmd: 'vc new', args: [{ n: 'kind', t: 'api|test|load', v: 'test' }, { n: 'file', t: 'file id', v: 'users.crud' }], desc: 'Scaffold a new item from the conventions.', preview: 'Adds a test to services/users/crud.test.yaml' }
  ]
};

/* Tests mode tree: services, then test files, then tests with last-run status */
VC.data.testsTree = [
  { service: 'users', files: [
    { id: 'users.crud', tests: [['createsUser', 'pass'], ['getsCreatedUser', 'pass'], ['rejectsBadEmail', 'flaky'], ['deletesUser', 'pass']] },
    { id: 'users.auth', tests: [['rejectsExpiredToken', 'pass'], ['refreshesToken', 'skipped']] }
  ] },
  { service: 'orders', files: [
    { id: 'orders.checkout', tests: [['placesOrder', 'pass'], ['rejectsDeclinedCard', 'fail'], ['refundsPartially', 'blocked']] }
  ] },
  { service: 'catalog', files: [
    { id: 'catalog.read', tests: [['readsProduct', 'allow'], ['hidesCostPrice', 'pass']] }
  ] },
  { service: 'rooms', files: [
    { id: 'rooms.lobby', tests: [['echoesChatMessage', 'skipped']] }
  ] }
];
