import { mount } from 'svelte';
import '../styles/base.css';
import './settings.css';
import Settings from './Settings.svelte';

mount(Settings, { target: document.getElementById('app')! });
