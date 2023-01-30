import react from '@vitejs/plugin-react';
import path from 'path';
import { defineConfig } from "vitest/config";

// https://vitejs.dev/config/
export default defineConfig({
    plugins: [react()],
    test: {
        environment: 'jsdom',
        globals: true,
    },
    resolve: {
        alias: {
            '@': path.resolve(__dirname, './src'),
            "@mu": path.resolve(__dirname, '../')
        }
    },
})