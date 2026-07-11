import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { pool } from './index.js';

const schemaPath = fileURLToPath(new URL('./schema.sql', import.meta.url));

async function migrate() {
  const schema = await readFile(schemaPath, 'utf8');
  await pool.query(schema);
  console.log('Schema applied.');
  await pool.end();
}

migrate().catch((err) => {
  console.error('Migration failed:', err);
  process.exit(1);
});
