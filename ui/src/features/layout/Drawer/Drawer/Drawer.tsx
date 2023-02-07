import {Sidebar} from "primereact/sidebar"

type DrawerWrapperProps = {
    isOpen: boolean;
    closeDrawer: () => void,
};

export default function Drawer({isOpen, closeDrawer}: DrawerWrapperProps) {
    return (
        <Sidebar visible={isOpen} onHide={closeDrawer}>
            <div data-testid="drawerContainer">
                drawer content
            </div>
        </Sidebar>
    );
}