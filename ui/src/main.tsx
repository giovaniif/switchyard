import { createRoot } from "react-dom/client";
import { App } from "./App";
import { phonePage } from "./mobile";
import { PhoneWelcome } from "./PhoneWelcome";
import "./styles.css";

// The setup QR code lands here on the iPhone: hand over to the app, not the whole UI.
const phone = phonePage(location.href);
createRoot(document.getElementById("root")!).render(phone ? <PhoneWelcome {...phone} /> : <App />);
