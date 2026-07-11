import Link from 'next/link';
import { WalletButton } from './WalletButton';

const LINKS = [
  { href: '/send', label: 'Send' },
  { href: '/swap', label: 'Swap' },
  { href: '/pool', label: 'Pool' },
  { href: '/cashout', label: 'Cash Out' },
];

export function NavBar() {
  return (
    <header className="border-b border-neutral-200 bg-white">
      <div className="mx-auto flex max-w-3xl items-center justify-between px-4 py-3">
        <Link href="/send" className="text-lg font-semibold text-brand-700">
          NairaBridge
        </Link>
        <nav className="flex gap-4 text-sm font-medium text-neutral-600">
          {LINKS.map((link) => (
            <Link key={link.href} href={link.href} className="hover:text-brand-700">
              {link.label}
            </Link>
          ))}
        </nav>
        <WalletButton />
      </div>
    </header>
  );
}
