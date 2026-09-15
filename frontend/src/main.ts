import './styles/app.css';
import { mount } from 'svelte';
import App from './App.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('找不到掛載節點 #app');

export default mount(App, { target });
