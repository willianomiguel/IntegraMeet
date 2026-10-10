/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      // As cores e fontes reais vêm de src/styles.css (variáveis CSS),
      // para que o app e a landing page usem a mesma identidade.
      fontFamily: {
        sans: ["Instrument Sans", "Segoe UI", "system-ui", "sans-serif"],
        display: ["Fraunces", "Iowan Old Style", "Georgia", "serif"],
      },
    },
  },
  plugins: [],
};
