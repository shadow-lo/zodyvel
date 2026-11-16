import { mount } from 'svelte';
import '../styles/base.css';
import Island from './Island.svelte';

mount(Island, { target: document.getElementById('app')! });
