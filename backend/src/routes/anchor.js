import { config } from '../config.js';

// SEP-10 authentication (proving control of the Stellar account) has to
// happen client-side, where the user's wallet can sign the challenge --
// these endpoints just take the resulting JWT and kick off the SEP-24
// interactive deposit/withdrawal flow with the anchor.

async function resolveTransferServer(domain) {
  const tomlRes = await fetch(`https://${domain}/.well-known/stellar.toml`);
  if (!tomlRes.ok) {
    throw Object.assign(new Error(`failed to fetch stellar.toml from ${domain}`), { status: 502 });
  }
  const toml = await tomlRes.text();
  const match = toml.match(/TRANSFER_SERVER_SEP0024\s*=\s*"([^"]+)"/);
  if (!match) {
    throw Object.assign(new Error(`${domain} does not advertise TRANSFER_SERVER_SEP0024`), {
      status: 502,
    });
  }
  return match[1];
}

async function startInteractive(kind, { transferServer, sep10Token, assetCode, account }) {
  const res = await fetch(`${transferServer}/transactions/${kind}/interactive`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${sep10Token}`,
    },
    body: JSON.stringify({ asset_code: assetCode, account }),
  });
  const body = await res.json();
  if (!res.ok) {
    throw Object.assign(new Error('anchor rejected interactive request'), { status: res.status, body });
  }
  return { url: body.url, id: body.id };
}

function registerInteractiveRoute(fastify, path, kind) {
  fastify.post(path, async (request, reply) => {
    const { account, assetCode, sep10Token, homeDomain } = request.body ?? {};
    if (!account || !assetCode || !sep10Token) {
      return reply.code(400).send({
        error:
          'account, assetCode, and sep10Token are required (complete SEP-10 auth with the wallet client-side first)',
      });
    }

    const domain = homeDomain ?? config.anchorHomeDomains[0];
    if (!domain) {
      return reply.code(503).send({ error: 'no anchor home domain configured' });
    }

    try {
      const transferServer = await resolveTransferServer(domain);
      const result = await startInteractive(kind, { transferServer, sep10Token, assetCode, account });
      return result;
    } catch (err) {
      return reply.code(err.status ?? 502).send({ error: err.body ?? err.message });
    }
  });
}

export default async function anchorRoutes(fastify) {
  registerInteractiveRoute(fastify, '/anchor/deposit-url', 'deposit');
  // Powers the /cashout page: bank-transfer withdrawal, converting the
  // user's stablecoin balance back to fiat via the anchor.
  registerInteractiveRoute(fastify, '/anchor/withdraw-url', 'withdraw');
}
