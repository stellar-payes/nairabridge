import Fastify from 'fastify';
import cors from '@fastify/cors';
import { config } from './config.js';
import quoteRoutes from './routes/quote.js';
import poolsRoutes from './routes/pools.js';
import ratesRoutes from './routes/rates.js';
import anchorRoutes from './routes/anchor.js';

const fastify = Fastify({
  logger: {
    level: config.logLevel,
    transport: process.env.NODE_ENV === 'production' ? undefined : { target: 'pino-pretty' },
  },
});

await fastify.register(cors, { origin: true });
fastify.get('/health', async () => ({ status: 'ok' }));
await fastify.register(quoteRoutes);
await fastify.register(poolsRoutes);
await fastify.register(ratesRoutes);
await fastify.register(anchorRoutes);

try {
  await fastify.listen({ port: config.port, host: config.host });
} catch (err) {
  fastify.log.error(err);
  process.exit(1);
}
