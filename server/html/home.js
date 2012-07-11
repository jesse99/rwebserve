"use strict";

window.onload = function()
{
	var timeElement = document.getElementById('uptime');
	timeElement.innerHTML = "whatever";
	
	var source = new EventSource('/uptime');
	source.addEventListener('message', function(event)
	{
		console.log('> received ' + event.data);
		timeElement.innerHTML = event.data;
	});
	
	source.addEventListener('open', function(event)
	{
		console.log('> stream opened');
	});
	
	source.addEventListener('error', function(event)
	{
		if (event.eventPhase == 2)
		{
			console.log('> stream closed');
		}
	});
}
