import Home from './index'
import '@testing-library/jest-dom'
import {render, screen} from '@testing-library/react'

describe('Home', () => {
    it('renders a heading', () => {
        render(<Home/>)
        const heading = screen.getByTestId('content')
        expect(heading).toBeInTheDocument()
    })
})