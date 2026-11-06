import '@samriddhi/ui/base.css'
import './app.css'
import { mount } from 'svelte'
import { ui } from '@samriddhi/ui'
import App from './App.svelte'

ui.start()

export default mount(App, { target: document.getElementById('app')! })
