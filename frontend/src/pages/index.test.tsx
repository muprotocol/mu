import Home from './index';
import {render, screen} from '@testing-library/react'
import {describe, test, expect} from "vitest"

describe('Home', () => {
    test('renders a heading', () => {
        render(<Home/>)
        const heading = screen.getByTestId('content')
        expect(heading).toBeDefined();
    })
})
