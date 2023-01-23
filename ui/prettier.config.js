module.exports = {
  trailingComma: "all",
  importOrder: [
    "^react(.*)",
    "next",
    "@mu/",
    "<THIRD_PARTY_MODULES>",
    "@mui/",
    "@/components/",
    "@/layouts/",
    "@/hooks/",
    "@/utils/",
    "@/(.*)",
    "^[./]",
  ],
  importOrderSeparation: true,
  importOrderSortSpecifiers: true,
};
