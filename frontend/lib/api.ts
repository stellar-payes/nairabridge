const API_URL = process.env.NEXT_PUBLIC_API_URL ?? 'http://localhost:4000';

export interface QuoteResponse {
  route: string[];
  pools: string[];
  amountIn: number;
  amountOut: number;
  priceImpact: number;
}

export interface PoolSummary {
  id: string;
  tokens: string[];
  tokenSymbols: string[];
  lpToken: string;
  amp: number;
  feeBps: number;
}

export interface PoolStats extends PoolSummary {
  reserves: number[];
  tvl: number;
  volume24h: number;
  apy: number;
}

async function api<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${API_URL}${path}`, {
    ...init,
    headers: { 'Content-Type': 'application/json', ...init?.headers },
  });
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new Error(body.error ? JSON.stringify(body.error) : `Request to ${path} failed (${res.status})`);
  }
  return res.json();
}

export function getQuote(inSymbol: string, outSymbol: string, amount: number): Promise<QuoteResponse> {
  const params = new URLSearchParams({ in: inSymbol, out: outSymbol, amount: String(amount) });
  return api<QuoteResponse>(`/quote?${params.toString()}`);
}

export function getPools(): Promise<PoolSummary[]> {
  return api<PoolSummary[]>('/pools');
}

export function getPoolStats(poolId: string): Promise<PoolStats> {
  return api<PoolStats>(`/pools/${poolId}/stats`);
}

interface AnchorInteractiveRequest {
  account: string;
  assetCode: string;
  sep10Token: string;
  homeDomain?: string;
}

export function getDepositUrl(body: AnchorInteractiveRequest): Promise<{ url: string; id: string }> {
  return api('/anchor/deposit-url', { method: 'POST', body: JSON.stringify(body) });
}

export function getWithdrawUrl(body: AnchorInteractiveRequest): Promise<{ url: string; id: string }> {
  return api('/anchor/withdraw-url', { method: 'POST', body: JSON.stringify(body) });
}
