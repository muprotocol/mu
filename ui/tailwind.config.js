/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        green: {
          mu: "#19e4cc",
        },
      },
    },
  },
  corePlugins: {
    preflight: false
  },
  plugins: [],
};
