import Image from "next/image";


import { MuLogo } from "@/components/muLogo/muLogo";

import Drawer from "@/features/layout/Drawer/Drawer/Drawer";
import useDrawer from "@/features/layout/Drawer/useDrawer";
import WalletButton from "@/features/wallet/WalletButton/WalletButton";

export default function Header() {
  const { isOpen, openDrawer, closeDrawer } = useDrawer();

  return (
    <header className="flex! container mx-auto flex items-center gap-5 p-5">
      <Drawer isOpen={isOpen} closeDrawer={closeDrawer} />
      <button className="lg:hidden" onClick={openDrawer}>
          <p>lol</p>
      </button>
      <div>
        <MuLogo />
      </div>
      <div className="ml-auto">
        <WalletButton />
      </div>
    </header>
  );
}
