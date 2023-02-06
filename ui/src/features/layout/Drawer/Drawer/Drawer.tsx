import {Sidebar} from "primereact/sidebar"

type DrawerWrapperProps = {
    isOpen: boolean;
    closeDrawer: () => void,
};

export default function Drawer({isOpen, closeDrawer}: DrawerWrapperProps) {
    return (
        <Sidebar visible={isOpen} onHide={closeDrawer}>
            drawer content
        </Sidebar>
    );
}