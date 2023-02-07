import {createContext, useState} from "react";

export default function useDrawer() {
  const [isOpen, setIsOpen] = useState<boolean>(false);

  const openDrawer = () => {
    setIsOpen(true);
  };

  const closeDrawer = () => {
    setIsOpen(false);
  };

  return {
    isOpen,
    openDrawer,
    closeDrawer,
  };
}
