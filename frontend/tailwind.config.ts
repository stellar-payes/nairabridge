import type { Config } from 'tailwindcss';

const config: Config = {
  content: ['./app/**/*.{ts,tsx}', './components/**/*.{ts,tsx}'],
  theme: {
    extend: {
      colors: {
        brand: {
          50: '#eefdf5',
          100: '#d5f9e3',
          500: '#0fa968',
          600: '#0b8a54',
          700: '#0a6e44',
        },
      },
    },
  },
  plugins: [],
};

export default config;
