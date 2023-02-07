import Image from "next/image";

import {Button} from "primereact/button"
import {MuLogo} from "@/components/muLogo/muLogo";

import Drawer from "@/features/layout/Drawer/Drawer/Drawer";
import useDrawer from "@/features/layout/Drawer/useDrawer";
import WalletButton from "@/features/wallet/WalletButton/WalletButton";

export default function Header() {
    const {isOpen, openDrawer, closeDrawer} = useDrawer();

    return (
        <header className="w-full bg-slate-100 mb-10 shadow">
            <div className="flex! container mx-auto flex items-center gap-5 py-5 ">
                <Drawer isOpen={isOpen} closeDrawer={closeDrawer}/>
                <Button icon="pi pi-bars !text-xl" className="p-button-rounded p-button-text lg:!hidden"
                        onClick={openDrawer}/>
                <div>
                    <MuLogo/>
                </div>
                <div className="ml-auto">
                    <WalletButton/>
                </div>
            </div>
        </header>
    );
}
