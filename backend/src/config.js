import 'dotenv/config';

function required(name, fallback) {
  const value = process.env[name] ?? fallback;
  if (value === undefined) {
    throw new Error(`Missing required env var: ${name}`);
  }
  return value;
}

export const config = {
  sorobanRpcUrl: required('SOROBAN_RPC_URL', 'https://soroban-testnet.stellar.org'),
  networkPassphrase: required('NETWORK_PASSPHRASE', 'Test SDF Network ; September 2015'),
  databaseUrl: required('DATABASE_URL', 'postgres://nairabridge:nairabridge@localhost:5432/nairabridge'),
  anchorHomeDomains: (process.env.ANCHOR_HOME_DOMAINS ?? 'testanchor.stellar.org')
    .split(',')
    .map((d) => d.trim())
    .filter(Boolean),
  routerContractId: process.env.ROUTER_CONTRACT_ID ?? null,
  factoryContractId: process.env.FACTORY_CONTRACT_ID ?? null,
  port: Number(process.env.PORT ?? 4000),
  host: process.env.HOST ?? '0.0.0.0',
  logLevel: process.env.LOG_LEVEL ?? 'info',
};
