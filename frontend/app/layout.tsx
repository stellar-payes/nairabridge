import type { Metadata } from 'next';
import { WalletProvider } from '@/components/WalletProvider';
import { NavBar } from '@/components/NavBar';
import './globals.css';

export const metadata: Metadata = {
  title: 'NairaBridge — Send Money Home',
  description: 'Swap and send stablecoins (USDC ⇄ NGNC ⇄ EURC) for near-zero fees.',
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>
        <WalletProvider>
          <NavBar />
          <main className="mx-auto max-w-3xl px-4 py-8">{children}</main>
        </WalletProvider>
      </body>
    </html>
  );
}
