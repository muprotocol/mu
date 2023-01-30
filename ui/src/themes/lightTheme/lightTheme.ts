import { createTheme } from "@mui/material/styles";

export const lightTheme = createTheme({
  breakpoints: {
    values: {
      xs: 0,
      sm: 640,
      md: 768,
      lg: 1024,
      xl: 1280,
    },
  },
});

// MUI Default Theme: https://mui.com/material-ui/customization/default-theme/
