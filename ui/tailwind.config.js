/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        mu: {
          primary: "#19e4cc",
        },
      },
    },
  },
  corePlugins: {
    preflight: false,
  },
  plugins: [],
};
